/**
 * ENI World Builder — Binary Download & Version Management
 *
 * Platform detection, path utilities, version comparison, and GitHub release
 * fetching/downloading for the sidecar binary.
 *
 * Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 7.2, 7.3, 7.4
 */

'use strict';

const path = require('path');
const fs = require('fs');
const https = require('https');
const { execFileSync } = require('child_process');
const config = require('./config');

/**
 * Set of supported platform-arch identifiers.
 * Each entry combines process.platform and process.arch.
 */
const SUPPORTED_PLATFORMS = new Set([
    'darwin-arm64',
    'darwin-x64',
    'linux-x64',
    'win32-x64',
]);

/**
 * Returns the platform-specific sidecar binary filename.
 *
 * Constructs the identifier from process.platform and process.arch,
 * validates against supported platforms, and appends .exe on Windows.
 *
 * @returns {string|null} Binary filename or null if platform is unsupported.
 */
function getBinaryName() {
    const platform = process.platform;
    const arch = process.arch;
    const identifier = `${platform}-${arch}`;

    if (!SUPPORTED_PLATFORMS.has(identifier)) {
        console.error(`[ENI] Unsupported platform: ${identifier}. Supported platforms: ${[...SUPPORTED_PLATFORMS].join(', ')}`);
        return null;
    }

    const baseName = `eni-sidecar-${platform}-${arch}`;
    return platform === 'win32' ? `${baseName}.exe` : baseName;
}

/**
 * Returns the full path to the sidecar binary in the bin/ directory.
 *
 * Uses path.join() for cross-platform path construction (Requirement 7.3).
 *
 * @returns {string|null} Full path to binary or null if platform is unsupported.
 */
function getBinaryPath() {
    const binaryName = getBinaryName();
    if (!binaryName) {
        return null;
    }
    return path.join(__dirname, '..', 'bin', binaryName);
}

/**
 * Runs the local sidecar binary with --version and parses the semver output.
 *
 * @returns {string|null} Version string (e.g., "1.2.3") or null if binary
 *   doesn't exist or version cannot be determined.
 */
function getLocalVersion() {
    try {
        const binaryPath = getBinaryPath();
        if (!binaryPath || !fs.existsSync(binaryPath)) {
            return null;
        }

        const output = execFileSync(binaryPath, ['--version'], {
            encoding: 'utf8',
            timeout: 5000,
        });

        // Extract semver pattern (major.minor.patch) from output
        const match = output.match(/(\d+\.\d+\.\d+)/);
        return match ? match[1] : null;
    } catch (err) {
        return null;
    }
}

/**
 * Compares two semver version strings numerically.
 *
 * Parses each version into [major, minor, patch] and compares component by component.
 * Returns 0 for invalid version strings (treats as equal).
 *
 * @param {string} a - First version string (e.g., "1.2.3")
 * @param {string} b - Second version string (e.g., "1.3.0")
 * @returns {number} -1 if a < b, 0 if a === b, 1 if a > b
 */
function compareVersions(a, b) {
    const semverRegex = /^(\d+)\.(\d+)\.(\d+)$/;
    const matchA = String(a).match(semverRegex);
    const matchB = String(b).match(semverRegex);

    if (!matchA || !matchB) {
        return 0;
    }

    const partsA = [parseInt(matchA[1], 10), parseInt(matchA[2], 10), parseInt(matchA[3], 10)];
    const partsB = [parseInt(matchB[1], 10), parseInt(matchB[2], 10), parseInt(matchB[3], 10)];

    for (let i = 0; i < 3; i++) {
        if (partsA[i] < partsB[i]) return -1;
        if (partsA[i] > partsB[i]) return 1;
    }

    return 0;
}

/** GitHub repository for release fetching */
const GITHUB_REPO = 'punkpeye/eni-world-builder';
const GITHUB_API_URL = `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`;

/**
 * Makes an HTTPS GET request and returns the response body.
 * Follows redirects (up to 5 hops).
 *
 * @param {string} url - The URL to fetch
 * @param {object} [options] - Additional request options
 * @param {number} [redirectCount=0] - Current redirect depth
 * @returns {Promise<{statusCode: number, body: string|Buffer, headers: object}>}
 */
function httpsGet(url, options = {}, redirectCount = 0) {
    return new Promise((resolve, reject) => {
        if (redirectCount > 5) {
            return reject(new Error('Too many redirects'));
        }

        const parsedUrl = new URL(url);
        const reqOptions = {
            hostname: parsedUrl.hostname,
            path: parsedUrl.pathname + parsedUrl.search,
            headers: {
                'User-Agent': 'eni-world-builder-plugin',
                ...options.headers,
            },
        };

        const req = https.get(reqOptions, (res) => {
            // Follow redirects
            if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
                return resolve(httpsGet(res.headers.location, options, redirectCount + 1));
            }

            const chunks = [];
            res.on('data', (chunk) => chunks.push(chunk));
            res.on('end', () => {
                const body = Buffer.concat(chunks);
                resolve({
                    statusCode: res.statusCode,
                    body: options.binary ? body : body.toString('utf-8'),
                    headers: res.headers,
                });
            });
        });

        req.on('error', reject);
        req.setTimeout(30000, () => {
            req.destroy(new Error('Request timed out'));
        });
    });
}

/**
 * Sleeps for the specified number of milliseconds.
 *
 * @param {number} ms - Milliseconds to wait
 * @returns {Promise<void>}
 */
function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Fetches the latest release information from GitHub API.
 *
 * Handles 403/429 responses with exponential backoff: waits 1s, 2s, 4s
 * (up to 3 retries) before giving up.
 *
 * @returns {Promise<{tag_name: string, assets: Array}|null>} Release object or null on failure.
 */
async function fetchLatestRelease() {
    const maxRetries = 3;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
        try {
            const response = await httpsGet(GITHUB_API_URL, {
                headers: { Accept: 'application/vnd.github.v3+json' },
            });

            if (response.statusCode === 200) {
                return JSON.parse(response.body);
            }

            if ((response.statusCode === 403 || response.statusCode === 429) && attempt < maxRetries) {
                const backoffMs = Math.pow(2, attempt) * 1000; // 1s, 2s, 4s
                console.log(`[ENI] GitHub API rate limited (${response.statusCode}), retrying in ${backoffMs / 1000}s...`);
                await sleep(backoffMs);
                continue;
            }

            console.error(`[ENI] GitHub API returned status ${response.statusCode}`);
            return null;
        } catch (err) {
            if (attempt < maxRetries) {
                const backoffMs = Math.pow(2, attempt) * 1000;
                console.log(`[ENI] GitHub API request failed, retrying in ${backoffMs / 1000}s...`);
                await sleep(backoffMs);
                continue;
            }
            console.error(`[ENI] Failed to fetch latest release: ${err.message}`);
            console.error(`[ENI] Manual download: https://github.com/${GITHUB_REPO}/releases/latest`);
            return null;
        }
    }

    return null;
}

/**
 * Downloads the sidecar binary from a GitHub release asset URL.
 *
 * After download, verifies file size (discards if < 50% of expected size and retries once).
 * On macOS/Linux, sets executable permission (0o755).
 *
 * @param {string} assetUrl - The browser_download_url of the asset
 * @param {number} expectedSize - Expected file size in bytes from the release asset
 * @returns {Promise<boolean>} true on success, false on failure
 */
async function downloadBinary(assetUrl, expectedSize) {
    const binaryPath = getBinaryPath();
    if (!binaryPath) {
        console.error('[ENI] Cannot download binary: unsupported platform');
        return false;
    }

    const binDir = path.dirname(binaryPath);

    // Ensure bin/ directory exists
    try {
        fs.mkdirSync(binDir, { recursive: true });
    } catch (err) {
        console.error(`[ENI] Failed to create bin directory: ${err.message}`);
        return false;
    }

    // Attempt download (with one retry on corruption)
    for (let attempt = 0; attempt < 2; attempt++) {
        try {
            const response = await httpsGet(assetUrl, { binary: true, headers: { Accept: 'application/octet-stream' } });

            if (response.statusCode !== 200) {
                console.error(`[ENI] Download failed with status ${response.statusCode}`);
                console.error(`[ENI] Manual download: ${assetUrl}`);
                return false;
            }

            const fileBuffer = response.body;

            // Verify file size: discard if < 50% of expected
            if (expectedSize > 0 && fileBuffer.length < expectedSize * 0.5) {
                console.error(`[ENI] Downloaded file size (${fileBuffer.length} bytes) is less than 50% of expected (${expectedSize} bytes)`);
                if (attempt === 0) {
                    console.log('[ENI] Retrying download due to possible corruption...');
                    continue;
                }
                console.error('[ENI] Download retry also produced a corrupted file');
                return false;
            }

            // Write the binary to disk
            fs.writeFileSync(binaryPath, fileBuffer);

            // Set executable permission on macOS/Linux
            if (process.platform !== 'win32') {
                fs.chmodSync(binaryPath, 0o755);
            }

            console.log(`[ENI] Binary downloaded successfully: ${path.basename(binaryPath)}`);
            return true;
        } catch (err) {
            if (attempt === 0) {
                console.error(`[ENI] Download error: ${err.message}, retrying...`);
                continue;
            }
            console.error(`[ENI] Failed to download binary: ${err.message}`);
            console.error(`[ENI] Manual download: https://github.com/${GITHUB_REPO}/releases/latest`);
            return false;
        }
    }

    return false;
}

/**
 * Checks for a newer version of the sidecar binary and downloads it if available.
 *
 * Compares the local binary version against the latest GitHub release.
 * If the binary doesn't exist locally, always downloads.
 * Respects the autoUpdate config setting.
 *
 * @returns {Promise<{updated: boolean, version: string|null}>}
 */
async function checkForUpdate() {
    const binaryPath = getBinaryPath();
    const binaryExists = binaryPath && fs.existsSync(binaryPath);
    const localVersion = getLocalVersion();

    // If binary exists and autoUpdate is disabled, skip
    if (binaryExists && !config.autoUpdate) {
        return { updated: false, version: localVersion };
    }

    const release = await fetchLatestRelease();
    if (!release) {
        return { updated: false, version: localVersion };
    }

    // Extract version from tag_name (strip leading 'v' if present)
    const remoteVersion = release.tag_name ? release.tag_name.replace(/^v/, '') : null;
    if (!remoteVersion) {
        console.error('[ENI] Could not determine remote version from release tag');
        return { updated: false, version: localVersion };
    }

    // If binary doesn't exist locally, always download
    if (!binaryExists) {
        console.log(`[ENI] No local binary found, downloading v${remoteVersion}...`);
    } else if (localVersion && compareVersions(localVersion, remoteVersion) >= 0) {
        // Local version is same or newer — no update needed
        return { updated: false, version: localVersion };
    } else {
        console.log(`[ENI] Update available: ${localVersion || 'unknown'} → ${remoteVersion}`);
    }

    // Find the correct asset for this platform
    const binaryName = getBinaryName();
    if (!binaryName) {
        return { updated: false, version: localVersion };
    }

    const asset = release.assets.find((a) => a.name === binaryName);
    if (!asset) {
        console.error(`[ENI] No matching asset found for ${binaryName} in release ${release.tag_name}`);
        console.error(`[ENI] Available assets: ${release.assets.map((a) => a.name).join(', ')}`);
        return { updated: false, version: localVersion };
    }

    const success = await downloadBinary(asset.browser_download_url, asset.size);
    if (success) {
        return { updated: true, version: remoteVersion };
    }

    return { updated: false, version: localVersion };
}

module.exports = {
    SUPPORTED_PLATFORMS,
    getBinaryName,
    getBinaryPath,
    getLocalVersion,
    compareVersions,
    fetchLatestRelease,
    downloadBinary,
    checkForUpdate,
};
