/**
 * ENI World Builder — Sidecar Process Lifecycle Manager
 *
 * Spawns the Rust sidecar binary, monitors health, manages the child process,
 * handles graceful shutdown, and provides crash recovery with rate limiting.
 *
 * Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 6.4, 7.1
 */

'use strict';

const { spawn: cpSpawn } = require('child_process');
const http = require('http');
const config = require('./config');
const { getBinaryPath, getLocalVersion } = require('./binary');

/** Internal reference to the spawned child process */
let sidecarProcess = null;

/** Flag to prevent restart during intentional shutdown */
let crashRecoveryEnabled = false;

/** Array of restart timestamps for sliding window rate limiter */
let restartTimestamps = [];

/**
 * Checks if the sidecar is already running by hitting the health endpoint.
 * Uses a short 2-second timeout to avoid blocking initialization.
 *
 * @returns {Promise<boolean>} true if sidecar responds with 200, false otherwise
 */
function isAlreadyRunning() {
    return new Promise((resolve) => {
        const req = http.get(
            `http://127.0.0.1:${config.sidecarPort}/health`,
            { timeout: 2000 },
            (res) => {
                // Consume response data to free up memory
                res.resume();
                resolve(res.statusCode === 200);
            }
        );

        req.on('error', () => {
            resolve(false);
        });

        req.on('timeout', () => {
            req.destroy();
            resolve(false);
        });
    });
}

/**
 * Polls the health endpoint until a 200 response is received or timeout is reached.
 * Polls every 500ms for up to 10 seconds (20 attempts).
 *
 * @returns {Promise<boolean>} true if health check passes, false on timeout
 */
function waitForHealth() {
    const maxAttempts = 20;
    const intervalMs = 500;
    let attempts = 0;

    return new Promise((resolve) => {
        const poll = () => {
            attempts++;

            const req = http.get(
                `http://127.0.0.1:${config.sidecarPort}/health`,
                { timeout: 2000 },
                (res) => {
                    res.resume();
                    if (res.statusCode === 200) {
                        resolve(true);
                    } else if (attempts >= maxAttempts) {
                        resolve(false);
                    } else {
                        setTimeout(poll, intervalMs);
                    }
                }
            );

            req.on('error', () => {
                if (attempts >= maxAttempts) {
                    resolve(false);
                } else {
                    setTimeout(poll, intervalMs);
                }
            });

            req.on('timeout', () => {
                req.destroy();
                if (attempts >= maxAttempts) {
                    resolve(false);
                } else {
                    setTimeout(poll, intervalMs);
                }
            });
        };

        poll();
    });
}

/**
 * Spawns the sidecar binary as a child process.
 *
 * Checks if the binary exists, whether the sidecar is already running,
 * and then spawns the process with the configured port. Waits for the
 * health endpoint to respond before returning.
 *
 * @returns {Promise<boolean>} true if sidecar is running and healthy, false otherwise
 */
async function spawn() {
    const binaryPath = getBinaryPath();

    // Check if binary exists
    if (!binaryPath) {
        console.error('[ENI] Cannot spawn sidecar: binary path could not be determined (unsupported platform)');
        return false;
    }

    const fs = require('fs');
    if (!fs.existsSync(binaryPath)) {
        console.error(`[ENI] Cannot spawn sidecar: binary not found at ${binaryPath}`);
        return false;
    }

    // Check if already running
    const alreadyRunning = await isAlreadyRunning();
    if (alreadyRunning) {
        console.log('[ENI] Existing sidecar instance found, skipping spawn');
        return true;
    }

    // Spawn the child process
    const port = config.sidecarPort;
    console.log(`[ENI] Spawning sidecar on port ${port}...`);

    try {
        sidecarProcess = cpSpawn(binaryPath, ['--port', String(port)], {
            stdio: 'pipe',
        });

        // Handle stdout — prefix with [ENI]
        if (sidecarProcess.stdout) {
            sidecarProcess.stdout.on('data', (data) => {
                const lines = data.toString().trim().split('\n');
                for (const line of lines) {
                    if (line) {
                        console.log(`[ENI] sidecar: ${line}`);
                    }
                }
            });
        }

        // Handle stderr — prefix with [ENI]
        if (sidecarProcess.stderr) {
            sidecarProcess.stderr.on('data', (data) => {
                const lines = data.toString().trim().split('\n');
                for (const line of lines) {
                    if (line) {
                        console.error(`[ENI] sidecar: ${line}`);
                    }
                }
            });
        }

        // Handle spawn errors
        sidecarProcess.on('error', (err) => {
            console.error(`[ENI] Sidecar process error: ${err.message}`);
            sidecarProcess = null;
        });

        // Wait for health check
        const healthy = await waitForHealth();
        if (healthy) {
            console.log('[ENI] Sidecar is healthy and ready');
        } else {
            console.error('[ENI] Sidecar failed to become healthy within 10 seconds');
        }

        return healthy;
    } catch (err) {
        console.error(`[ENI] Failed to spawn sidecar: ${err.message}`);
        sidecarProcess = null;
        return false;
    }
}

/**
 * Returns the internal child process reference.
 * Used by the shutdown/crash recovery module.
 *
 * @returns {import('child_process').ChildProcess|null}
 */
function getProcess() {
    return sidecarProcess;
}

/**
 * Sets the internal child process reference.
 * Used by the shutdown/crash recovery module.
 *
 * @param {import('child_process').ChildProcess|null} proc
 */
function setProcess(proc) {
    sidecarProcess = proc;
}

/**
 * Gracefully kills the sidecar process.
 *
 * Sends SIGTERM (or taskkill on Windows), waits up to 5 seconds for the process
 * to exit, then sends SIGKILL (or taskkill /f on Windows) if still running.
 * Disables crash recovery to prevent restart during intentional shutdown.
 *
 * @returns {Promise<void>} Resolves when the process is terminated.
 */
function kill() {
    const proc = getProcess();
    if (!proc) {
        return Promise.resolve();
    }

    // Disable crash recovery during intentional shutdown
    crashRecoveryEnabled = false;

    const pid = proc.pid;

    return new Promise((resolve) => {
        let exited = false;

        const onExit = () => {
            if (!exited) {
                exited = true;
                sidecarProcess = null;
                resolve();
            }
        };

        proc.once('exit', onExit);

        // Send initial termination signal
        if (process.platform === 'win32') {
            try {
                cpSpawn('taskkill', ['/pid', String(pid), '/f']);
            } catch (err) {
                console.error(`[ENI] taskkill failed: ${err.message}`);
            }
        } else {
            try {
                proc.kill('SIGTERM');
            } catch (err) {
                console.error(`[ENI] SIGTERM failed: ${err.message}`);
            }
        }

        // Wait 5 seconds, then force kill if still running
        setTimeout(() => {
            if (!exited) {
                console.log('[ENI] Sidecar did not exit within 5s, sending SIGKILL');
                if (process.platform === 'win32') {
                    try {
                        cpSpawn('taskkill', ['/pid', String(pid), '/f']);
                    } catch (err) {
                        console.error(`[ENI] Force taskkill failed: ${err.message}`);
                    }
                } else {
                    try {
                        proc.kill('SIGKILL');
                    } catch (err) {
                        console.error(`[ENI] SIGKILL failed: ${err.message}`);
                    }
                }

                // Give a short grace period after force kill, then resolve anyway
                setTimeout(() => {
                    if (!exited) {
                        exited = true;
                        sidecarProcess = null;
                        resolve();
                    }
                }, 1000);
            }
        }, 5000);
    });
}

/**
 * Checks the sliding window rate limiter to determine if a restart is allowed.
 *
 * Prunes timestamps older than 60 seconds. If 5 or more timestamps remain
 * after pruning, the restart is suppressed.
 *
 * @returns {boolean} true if restart is allowed, false if rate limited
 */
function isRestartAllowed() {
    const now = Date.now();
    const windowMs = 60000; // 60 seconds

    // Prune timestamps older than 60 seconds
    restartTimestamps = restartTimestamps.filter((ts) => (now - ts) < windowMs);

    // Suppress if 5 or more restarts in the window
    if (restartTimestamps.length >= 5) {
        return false;
    }

    return true;
}

/**
 * Enables crash recovery monitoring on the current child process.
 *
 * Listens for the 'exit' event on the child process. When the process exits
 * unexpectedly (and crash recovery is enabled), checks the rate limiter and
 * attempts a restart after a 2-second delay.
 */
function enableCrashRecovery() {
    const proc = getProcess();
    if (!proc) {
        return;
    }

    crashRecoveryEnabled = true;

    proc.on('exit', (code, signal) => {
        if (!crashRecoveryEnabled) {
            return;
        }

        console.log(`[ENI] Sidecar exited unexpectedly (code: ${code}, signal: ${signal})`);

        if (!isRestartAllowed()) {
            console.error('[ENI] Restart suppressed: too many restarts (5) within 60 seconds');
            return;
        }

        // Record this restart attempt
        restartTimestamps.push(Date.now());

        console.log('[ENI] Attempting restart in 2 seconds...');
        setTimeout(async () => {
            if (!crashRecoveryEnabled) {
                return;
            }
            sidecarProcess = null;
            const success = await spawn();
            if (success) {
                console.log('[ENI] Sidecar restarted successfully');
                enableCrashRecovery();
            } else {
                console.error('[ENI] Sidecar restart failed');
            }
        }, 2000);
    });
}

/**
 * Returns whether the sidecar process is currently running.
 *
 * @returns {boolean} true if the process exists and hasn't exited
 */
function isRunning() {
    const proc = getProcess();
    if (!proc) {
        return false;
    }
    // If the process has a pid and hasn't been killed, it's running
    // exitCode is null while the process is still running
    return proc.exitCode === null && proc.signalCode === null;
}

/**
 * Returns the sidecar version by querying the local binary.
 *
 * @returns {string|null} Version string or null if unavailable
 */
function version() {
    return getLocalVersion();
}

/**
 * Returns the PID of the running sidecar process.
 *
 * @returns {number|null} Process ID or null if not running
 */
function pid() {
    const proc = getProcess();
    return proc ? proc.pid : null;
}

/**
 * Resets the restart timestamps array.
 * Exposed for testing purposes.
 */
function resetRestartTimestamps() {
    restartTimestamps = [];
}

/**
 * Gets the current restart timestamps array.
 * Exposed for testing purposes.
 *
 * @returns {number[]}
 */
function getRestartTimestamps() {
    return restartTimestamps;
}

/**
 * Sets the restart timestamps array.
 * Exposed for testing purposes.
 *
 * @param {number[]} timestamps
 */
function setRestartTimestamps(timestamps) {
    restartTimestamps = timestamps;
}

module.exports = {
    spawn,
    waitForHealth,
    isAlreadyRunning,
    getProcess,
    setProcess,
    kill,
    enableCrashRecovery,
    isRunning,
    version,
    pid,
    isRestartAllowed,
    resetRestartTimestamps,
    getRestartTimestamps,
    setRestartTimestamps,
};
