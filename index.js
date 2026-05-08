/**
 * ENI World Builder — SillyTavern Server Plugin
 *
 * Manages the Rust sidecar binary lifecycle:
 * - Checks if the binary exists for the current platform
 * - Downloads it from GitHub Releases if missing
 * - Spawns the sidecar as a child process
 * - Health-checks, restarts on crash, kills on shutdown
 */

const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');

const SIDECAR_PORT = 7842;
const HEALTH_ENDPOINT = `http://127.0.0.1:${SIDECAR_PORT}/health`;
const GITHUB_REPO = 'joogleibooglei-web/AgentSilly';

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
 * Download the sidecar binary from the latest GitHub Release.
 */
async function downloadBinary(logger) {
    const binaryName = getBinaryName();
    const binDir = path.join(__dirname, 'bin');

    if (!fs.existsSync(binDir)) {
        fs.mkdirSync(binDir, { recursive: true });
    }

    const targetPath = getBinaryPath();

    try {
        logger.info(`[ENI] Fetching latest release from GitHub...`);
        const releaseUrl = `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`;
        const releaseRes = await fetch(releaseUrl, {
            headers: { 'Accept': 'application/vnd.github.v3+json' },
        });

        if (!releaseRes.ok) {
            throw new Error(`GitHub API returned ${releaseRes.status}: ${releaseRes.statusText}`);
        }

        const release = await releaseRes.json();
        const asset = release.assets.find(a => a.name === binaryName);

        if (!asset) {
            throw new Error(
                `No binary found for platform "${process.platform}-${process.arch}" in release ${release.tag_name}. ` +
                `Available assets: ${release.assets.map(a => a.name).join(', ')}`
            );
        }

        logger.info(`[ENI] Downloading ${asset.name} (${(asset.size / 1024 / 1024).toFixed(1)} MB)...`);
        const downloadRes = await fetch(asset.browser_download_url);

        if (!downloadRes.ok) {
            throw new Error(`Download failed: ${downloadRes.status}`);
        }

        const buffer = Buffer.from(await downloadRes.arrayBuffer());
        fs.writeFileSync(targetPath, buffer);

        // Set executable permissions on unix
        if (process.platform !== 'win32') {
            fs.chmodSync(targetPath, 0o755);
        }

        logger.info(`[ENI] Binary downloaded successfully: ${targetPath}`);
        return true;
    } catch (error) {
        // Clean up partial download
        if (fs.existsSync(targetPath)) {
            fs.unlinkSync(targetPath);
        }
        logger.error(`[ENI] Failed to download sidecar binary: ${error.message}`);
        return false;
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
 * SillyTavern extension entry point.
 */
async function init(app, logger) {
    logger.info('[ENI] World Builder server plugin initializing...');

    // Check if sidecar is already running
    if (await isAlreadyRunning()) {
        logger.info('[ENI] Sidecar already running on port ' + SIDECAR_PORT);
        return;
    }

    // Check if binary exists, download if not
    if (!binaryExists()) {
        logger.info('[ENI] Sidecar binary not found, attempting download...');
        const success = await downloadBinary(logger);
        if (!success) {
            logger.error('[ENI] Could not obtain sidecar binary. Extension will run without backend.');
            logger.error(`[ENI] Please download manually from: https://github.com/${GITHUB_REPO}/releases`);
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
