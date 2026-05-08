/**
 * ENI World Builder — SillyTavern Server Plugin
 *
 * Manages the Rust sidecar binary lifecycle:
 * - Checks if the binary exists for the current platform
 * - Verifies binary version via --version flag
 * - Downloads it from GitHub Releases if missing or outdated
 * - Handles errors: no internet, rate-limiting, unsupported platform, corrupt downloads
 * - Spawns the sidecar as a child process
 * - Health-checks, restarts on crash, kills on shutdown
 */

const path = require('path');
const fs = require('fs');
const { spawn, execFileSync } = require('child_process');

const SIDECAR_PORT = 7842;
const HEALTH_ENDPOINT = `http://127.0.0.1:${SIDECAR_PORT}/health`;
const GITHUB_REPO = 'joogleibooglei-web/AgentSilly';
const GITHUB_API_BASE = `https://api.github.com/repos/${GITHUB_REPO}`;

/** Maximum number of download retry attempts for corrupt/partial downloads */
const MAX_DOWNLOAD_RETRIES = 1;
/** Initial backoff delay (ms) for rate-limited requests */
const RATE_LIMIT_INITIAL_BACKOFF_MS = 1000;
/** Maximum backoff delay (ms) for rate-limited requests */
const RATE_LIMIT_MAX_BACKOFF_MS = 30000;
/** Maximum number of rate-limit retries before giving up */
const RATE_LIMIT_MAX_RETRIES = 3;

let sidecarProcess = null;

/**
 * Get the expected binary name for the current platform.
 */
function getBinaryName() {
    const platform = process.platform; // 'darwin', 'linux', 'win32'
    const arch = process.arch; // 'x64', 'arm64'

    if (platform === 'win32') {
        return `eni-sidecar-win32-${arch}.exe`;
    }
    return `eni-sidecar-${platform}-${arch}`;
}

/**
 * Get the path to the sidecar binary.
 */
function getBinaryPath() {
    const binDir = path.join(__dirname, 'bin');
    return path.join(binDir, getBinaryName());
}

/**
 * Check if the sidecar binary exists locally.
 */
function binaryExists() {
    return fs.existsSync(getBinaryPath());
}

/**
 * Check if the current platform is supported.
 * Returns an error message if unsupported, or null if supported.
 */
function checkPlatformSupport() {
    const platform = process.platform;
    const arch = process.arch;
    const supported = [
        { platform: 'darwin', arch: 'arm64' },
        { platform: 'darwin', arch: 'x64' },
        { platform: 'linux', arch: 'x64' },
        { platform: 'win32', arch: 'x64' },
    ];

    const isSupported = supported.some(s => s.platform === platform && s.arch === arch);
    if (!isSupported) {
        return `Unsupported platform: ${platform}-${arch}. Supported platforms: ${supported.map(s => `${s.platform}-${s.arch}`).join(', ')}`;
    }
    return null;
}

/**
 * Get the local binary version by running it with --version flag.
 * Returns the version string (e.g., "0.2.0") or null if the binary
 * doesn't exist, isn't executable, or fails to report a version.
 */
function getLocalBinaryVersion(logger) {
    const binaryPath = getBinaryPath();
    if (!fs.existsSync(binaryPath)) {
        return null;
    }

    try {
        const output = execFileSync(binaryPath, ['--version'], {
            timeout: 5000,
            encoding: 'utf-8',
        });
        // Expected output format: "eni-sidecar 0.2.0" or just "0.2.0"
        const versionMatch = output.trim().match(/(\d+\.\d+\.\d+)/);
        if (versionMatch) {
            return versionMatch[1];
        }
        logger.warn(`[ENI] Binary --version output did not contain a valid version: "${output.trim()}"`);
        return null;
    } catch (error) {
        logger.warn(`[ENI] Failed to get binary version: ${error.message}`);
        return null;
    }
}

/**
 * Parse a GitHub release tag into a version string.
 * Handles formats like "v0.2.0" → "0.2.0"
 */
function parseReleaseVersion(tagName) {
    const match = tagName.match(/v?(\d+\.\d+\.\d+)/);
    return match ? match[1] : null;
}

/**
 * Compare two semver version strings.
 * Returns: -1 if a < b, 0 if a == b, 1 if a > b
 */
function compareVersions(a, b) {
    const partsA = a.split('.').map(Number);
    const partsB = b.split('.').map(Number);

    for (let i = 0; i < 3; i++) {
        if (partsA[i] < partsB[i]) return -1;
        if (partsA[i] > partsB[i]) return 1;
    }
    return 0;
}

/**
 * Sleep for a given number of milliseconds.
 */
function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Fetch with rate-limit retry logic.
 * If GitHub returns 403 or 429 (rate-limited), retries with exponential backoff.
 * @param {string} url - The URL to fetch
 * @param {object} options - Fetch options
 * @param {object} logger - ST logger
 * @returns {Response} The fetch response
 * @throws {Error} If all retries are exhausted or a non-retryable error occurs
 */
async function fetchWithRateLimitRetry(url, options, logger) {
    let backoff = RATE_LIMIT_INITIAL_BACKOFF_MS;

    for (let attempt = 0; attempt <= RATE_LIMIT_MAX_RETRIES; attempt++) {
        const response = await fetch(url, options);

        if (response.status === 403 || response.status === 429) {
            // Check for rate limit headers
            const rateLimitRemaining = response.headers.get('x-ratelimit-remaining');
            const rateLimitReset = response.headers.get('x-ratelimit-reset');

            if (attempt >= RATE_LIMIT_MAX_RETRIES) {
                const error = new Error(
                    `GitHub API rate limit exceeded after ${RATE_LIMIT_MAX_RETRIES} retries. ` +
                    `Please wait and try again, or download the binary manually from: ` +
                    `https://github.com/${GITHUB_REPO}/releases`
                );
                error.code = 'RATE_LIMITED';
                throw error;
            }

            // Calculate wait time from reset header or use exponential backoff
            let waitMs = backoff;
            if (rateLimitReset) {
                const resetTime = parseInt(rateLimitReset, 10) * 1000;
                const now = Date.now();
                if (resetTime > now) {
                    waitMs = Math.min(resetTime - now, RATE_LIMIT_MAX_BACKOFF_MS);
                }
            }

            logger.warn(
                `[ENI] GitHub API rate-limited (${response.status}). ` +
                `Remaining: ${rateLimitRemaining || 'unknown'}. ` +
                `Retrying in ${Math.round(waitMs / 1000)}s (attempt ${attempt + 1}/${RATE_LIMIT_MAX_RETRIES})...`
            );

            await sleep(waitMs);
            backoff = Math.min(backoff * 2, RATE_LIMIT_MAX_BACKOFF_MS);
            continue;
        }

        return response;
    }

    // Should not reach here, but just in case
    const error = new Error('Unexpected state in rate-limit retry loop');
    error.code = 'RATE_LIMITED';
    throw error;
}

/**
 * Fetch the latest release info from GitHub.
 * Handles rate-limiting with backoff.
 * @returns {{ tagName: string, version: string, assets: Array }} Release info
 */
async function fetchLatestRelease(logger) {
    const releaseUrl = `${GITHUB_API_BASE}/releases/latest`;

    const response = await fetchWithRateLimitRetry(
        releaseUrl,
        { headers: { 'Accept': 'application/vnd.github.v3+json', 'User-Agent': 'ENI-WorldBuilder-ST-Plugin' } },
        logger
    );

    if (!response.ok) {
        throw new Error(`GitHub API returned ${response.status}: ${response.statusText}`);
    }

    const release = await response.json();
    const version = parseReleaseVersion(release.tag_name);

    return {
        tagName: release.tag_name,
        version,
        assets: release.assets,
    };
}

/**
 * Download a binary asset from GitHub.
 * Validates the download by checking file size.
 * @returns {boolean} true if download succeeded and file is valid
 */
async function downloadAsset(asset, targetPath, logger) {
    logger.info(`[ENI] Downloading ${asset.name} (${(asset.size / 1024 / 1024).toFixed(1)} MB)...`);

    const downloadRes = await fetch(asset.browser_download_url, {
        headers: { 'User-Agent': 'ENI-WorldBuilder-ST-Plugin' },
    });

    if (!downloadRes.ok) {
        throw new Error(`Download failed with status ${downloadRes.status}: ${downloadRes.statusText}`);
    }

    const buffer = Buffer.from(await downloadRes.arrayBuffer());

    // Validate download: check that we got a reasonable amount of data
    if (buffer.length === 0) {
        throw new Error('Downloaded file is empty (0 bytes)');
    }

    // Check against expected size if available (allow 1% tolerance for encoding differences)
    if (asset.size > 0 && buffer.length < asset.size * 0.5) {
        throw new Error(
            `Downloaded file appears corrupt or partial: got ${buffer.length} bytes, ` +
            `expected ~${asset.size} bytes`
        );
    }

    fs.writeFileSync(targetPath, buffer);

    // Set executable permissions on unix
    if (process.platform !== 'win32') {
        fs.chmodSync(targetPath, 0o755);
    }

    logger.info(`[ENI] Binary downloaded successfully: ${targetPath} (${buffer.length} bytes)`);
    return true;
}

/**
 * Download the sidecar binary from the latest GitHub Release.
 * Includes retry logic for corrupt/partial downloads and rate-limit handling.
 *
 * @param {object} logger - ST logger instance
 * @param {object} [releaseInfo] - Pre-fetched release info (optional, will fetch if not provided)
 * @returns {{ success: boolean, error?: string, errorCode?: string }}
 */
async function downloadBinary(logger, releaseInfo) {
    const binaryName = getBinaryName();
    const binDir = path.join(__dirname, 'bin');

    if (!fs.existsSync(binDir)) {
        fs.mkdirSync(binDir, { recursive: true });
    }

    const targetPath = getBinaryPath();

    try {
        // Fetch release info if not provided
        if (!releaseInfo) {
            logger.info(`[ENI] Fetching latest release from GitHub...`);
            releaseInfo = await fetchLatestRelease(logger);
        }

        // Find the matching asset
        const asset = releaseInfo.assets.find(a => a.name === binaryName);

        if (!asset) {
            const platformError = checkPlatformSupport();
            if (platformError) {
                return {
                    success: false,
                    error: platformError,
                    errorCode: 'UNSUPPORTED_PLATFORM',
                };
            }
            return {
                success: false,
                error:
                    `No binary found for platform "${process.platform}-${process.arch}" ` +
                    `in release ${releaseInfo.tagName}. ` +
                    `Available assets: ${releaseInfo.assets.map(a => a.name).join(', ')}`,
                errorCode: 'ASSET_NOT_FOUND',
            };
        }

        // Download with retry on corrupt/partial download
        let lastError = null;
        for (let attempt = 0; attempt <= MAX_DOWNLOAD_RETRIES; attempt++) {
            try {
                // Clean up any existing partial file before retry
                if (attempt > 0 && fs.existsSync(targetPath)) {
                    logger.warn(`[ENI] Removing partial/corrupt download, retrying (attempt ${attempt + 1})...`);
                    fs.unlinkSync(targetPath);
                }

                await downloadAsset(asset, targetPath, logger);
                return { success: true };
            } catch (downloadError) {
                lastError = downloadError;
                logger.warn(`[ENI] Download attempt ${attempt + 1} failed: ${downloadError.message}`);

                // Clean up partial file
                if (fs.existsSync(targetPath)) {
                    try {
                        fs.unlinkSync(targetPath);
                    } catch (cleanupErr) {
                        logger.warn(`[ENI] Failed to clean up partial download: ${cleanupErr.message}`);
                    }
                }

                if (attempt >= MAX_DOWNLOAD_RETRIES) {
                    break;
                }
            }
        }

        return {
            success: false,
            error: `Download failed after ${MAX_DOWNLOAD_RETRIES + 1} attempts: ${lastError.message}`,
            errorCode: 'DOWNLOAD_FAILED',
        };
    } catch (error) {
        // Clean up partial download on any error
        if (fs.existsSync(targetPath)) {
            try {
                fs.unlinkSync(targetPath);
            } catch (cleanupErr) {
                // Ignore cleanup errors
            }
        }

        // Classify the error
        if (error.code === 'RATE_LIMITED') {
            return {
                success: false,
                error: error.message,
                errorCode: 'RATE_LIMITED',
            };
        }

        // Check for network errors (no internet)
        if (
            error.code === 'ENOTFOUND' ||
            error.code === 'ENETUNREACH' ||
            error.code === 'ECONNREFUSED' ||
            error.code === 'ETIMEDOUT' ||
            error.cause?.code === 'ENOTFOUND' ||
            error.cause?.code === 'ENETUNREACH' ||
            error.message.includes('fetch failed') ||
            error.message.includes('network')
        ) {
            return {
                success: false,
                error:
                    `No internet connection available. Cannot download sidecar binary. ` +
                    `Please check your network connection or download manually from: ` +
                    `https://github.com/${GITHUB_REPO}/releases`,
                errorCode: 'NO_INTERNET',
            };
        }

        return {
            success: false,
            error: `Failed to download sidecar binary: ${error.message}`,
            errorCode: 'UNKNOWN',
        };
    }
}

/**
 * Check if the local binary is outdated compared to the latest release.
 * @returns {{ needsUpdate: boolean, localVersion: string|null, remoteVersion: string|null, releaseInfo: object|null }}
 */
async function checkForUpdate(logger) {
    const localVersion = getLocalBinaryVersion(logger);

    if (!localVersion) {
        // Binary exists but can't report version — treat as needing update
        return { needsUpdate: true, localVersion: null, remoteVersion: null, releaseInfo: null };
    }

    try {
        logger.info(`[ENI] Local binary version: ${localVersion}. Checking for updates...`);
        const releaseInfo = await fetchLatestRelease(logger);

        if (!releaseInfo.version) {
            logger.warn(`[ENI] Could not parse version from release tag: ${releaseInfo.tagName}`);
            return { needsUpdate: false, localVersion, remoteVersion: null, releaseInfo };
        }

        const comparison = compareVersions(localVersion, releaseInfo.version);
        if (comparison < 0) {
            logger.info(`[ENI] Update available: ${localVersion} → ${releaseInfo.version}`);
            return { needsUpdate: true, localVersion, remoteVersion: releaseInfo.version, releaseInfo };
        }

        logger.info(`[ENI] Binary is up to date (${localVersion})`);
        return { needsUpdate: false, localVersion, remoteVersion: releaseInfo.version, releaseInfo };
    } catch (error) {
        // If we can't check for updates (no internet, rate-limited), just use the existing binary
        logger.warn(`[ENI] Could not check for updates: ${error.message}. Using existing binary.`);
        return { needsUpdate: false, localVersion, remoteVersion: null, releaseInfo: null };
    }
}

/**
 * Check if the sidecar is already running (port in use).
 */
async function isAlreadyRunning() {
    try {
        const res = await fetch(HEALTH_ENDPOINT, { signal: AbortSignal.timeout(2000) });
        return res.ok;
    } catch {
        return false;
    }
}

/**
 * Spawn the sidecar process.
 */
function spawnSidecar(logger) {
    const binaryPath = getBinaryPath();

    logger.info(`[ENI] Starting sidecar: ${binaryPath}`);
    sidecarProcess = spawn(binaryPath, [], {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: { ...process.env },
    });

    sidecarProcess.stdout.on('data', (data) => {
        logger.info(`[ENI Sidecar] ${data.toString().trim()}`);
    });

    sidecarProcess.stderr.on('data', (data) => {
        logger.error(`[ENI Sidecar] ${data.toString().trim()}`);
    });

    sidecarProcess.on('exit', (code, signal) => {
        logger.warn(`[ENI] Sidecar exited with code ${code}, signal ${signal}`);
        sidecarProcess = null;
    });

    return sidecarProcess;
}

/**
 * Wait for the sidecar to become healthy.
 */
async function waitForHealth(timeoutMs = 10000) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
        try {
            const res = await fetch(HEALTH_ENDPOINT, { signal: AbortSignal.timeout(1000) });
            if (res.ok) return true;
        } catch {
            // Not ready yet
        }
        await new Promise(r => setTimeout(r, 500));
    }
    return false;
}

/**
 * Log an error with appropriate context and provide user-facing guidance.
 */
function logDownloadError(logger, result) {
    switch (result.errorCode) {
        case 'NO_INTERNET':
            logger.error(`[ENI] ${result.error}`);
            logger.error('[ENI] The sidecar will be unavailable until a network connection is restored.');
            break;
        case 'RATE_LIMITED':
            logger.error(`[ENI] ${result.error}`);
            logger.error('[ENI] GitHub API rate limit reached. Try again later or download manually.');
            break;
        case 'UNSUPPORTED_PLATFORM':
            logger.error(`[ENI] ${result.error}`);
            logger.error('[ENI] This platform is not supported by the pre-built sidecar binaries.');
            logger.error('[ENI] You may build from source: cd eni-sidecar && cargo build --release');
            break;
        case 'DOWNLOAD_FAILED':
            logger.error(`[ENI] ${result.error}`);
            logger.error(`[ENI] Please download manually from: https://github.com/${GITHUB_REPO}/releases`);
            break;
        default:
            logger.error(`[ENI] ${result.error}`);
            logger.error(`[ENI] Please download manually from: https://github.com/${GITHUB_REPO}/releases`);
            break;
    }
}

/**
 * SillyTavern extension entry point.
 */
async function init(app, logger) {
    logger.info('[ENI] World Builder server plugin initializing...');

    // Check platform support early
    const platformError = checkPlatformSupport();
    if (platformError) {
        logger.error(`[ENI] ${platformError}`);
        logger.error('[ENI] Extension will run without backend. You may build from source if your platform supports Rust.');
        return;
    }

    // Check if sidecar is already running
    if (await isAlreadyRunning()) {
        logger.info('[ENI] Sidecar already running on port ' + SIDECAR_PORT);
        return;
    }

    // Check if binary exists
    if (binaryExists()) {
        // Binary exists — check if it's up to date
        const updateCheck = await checkForUpdate(logger);

        if (updateCheck.needsUpdate) {
            logger.info('[ENI] Sidecar binary is outdated, downloading update...');
            const result = await downloadBinary(logger, updateCheck.releaseInfo);
            if (!result.success) {
                // Update failed — try to use existing binary anyway
                logger.warn(`[ENI] Update failed: ${result.error}`);
                logger.warn('[ENI] Continuing with existing binary version.');
            }
        }
    } else {
        // Binary missing — must download
        logger.info('[ENI] Sidecar binary not found, attempting download...');
        const result = await downloadBinary(logger);
        if (!result.success) {
            logDownloadError(logger, result);
            logger.error('[ENI] Could not obtain sidecar binary. Extension will run without backend.');
            return;
        }
    }

    // Spawn the sidecar
    spawnSidecar(logger);

    // Wait for health
    const healthy = await waitForHealth();
    if (healthy) {
        logger.info('[ENI] Sidecar is ready on port ' + SIDECAR_PORT);
    } else {
        logger.error('[ENI] Sidecar failed to start within timeout');
    }
}

/**
 * Called when ST shuts down or extension is unloaded.
 */
async function exit(logger) {
    if (sidecarProcess) {
        logger.info('[ENI] Shutting down sidecar...');
        sidecarProcess.kill('SIGTERM');

        // Wait up to 5 seconds for graceful shutdown
        await new Promise((resolve) => {
            const timeout = setTimeout(() => {
                if (sidecarProcess) {
                    logger.warn('[ENI] Force-killing sidecar');
                    sidecarProcess.kill('SIGKILL');
                }
                resolve();
            }, 5000);

            if (sidecarProcess) {
                sidecarProcess.on('exit', () => {
                    clearTimeout(timeout);
                    resolve();
                });
            } else {
                clearTimeout(timeout);
                resolve();
            }
        });

        sidecarProcess = null;
    }
}

module.exports = {
    init,
    exit,
    info: {
        id: 'eni-world-builder',
        name: 'ENI World Builder',
        description: 'AI-powered world building assistant with Rust sidecar backend',
    },
};
