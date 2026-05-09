/**
 * ENI World Builder — API Route Registration
 *
 * Registers plugin API routes on the Express router provided by SillyTavern.
 * Exposes sidecar status and restart functionality.
 *
 * Requirements: 5.1, 5.2, 5.3, 5.4
 */

'use strict';

const path = require('path');
const fs = require('fs');
const sidecar = require('./sidecar');

/**
 * Read the plugin version from plugin.json.
 *
 * @returns {string} Plugin version string
 */
function getPluginVersion() {
    try {
        const pluginJsonPath = path.join(__dirname, '..', 'plugin.json');
        const raw = fs.readFileSync(pluginJsonPath, 'utf-8');
        const manifest = JSON.parse(raw);
        return manifest.version || '0.0.0';
    } catch (err) {
        return '0.0.0';
    }
}

/**
 * Registers API routes on the provided Express router.
 *
 * Routes:
 *   GET  /status  — Returns sidecar running state and version info
 *   POST /restart — Kills and respawns the sidecar, returns new status
 *
 * @param {import('express').Router} router - Express router instance from SillyTavern
 */
function registerRoutes(router) {
    /**
     * GET /status
     *
     * Returns JSON with:
     *   - sidecar_running (boolean): whether the sidecar process is alive
     *   - sidecar_version (string|null): version string if running, null otherwise
     *   - plugin_version (string): version from plugin.json
     */
    router.get('/status', (req, res) => {
        const running = sidecar.isRunning();
        const sidecarVersion = sidecar.version();

        console.log(`[ENI] GET /status — sidecar_running: ${running}`);

        res.json({
            sidecar_running: running,
            sidecar_version: sidecarVersion,
            plugin_version: getPluginVersion(),
        });
    });

    /**
     * POST /restart
     *
     * Terminates the current sidecar process and spawns a new one.
     * Enables crash recovery after successful spawn.
     * Returns the new status in the same format as GET /status.
     * Returns 500 on error.
     */
    router.post('/restart', async (req, res) => {
        console.log('[ENI] POST /restart — restarting sidecar...');

        try {
            // Kill the current sidecar process
            await sidecar.kill();

            // Spawn a new sidecar instance
            await sidecar.spawn();

            // Enable crash recovery on the new process
            sidecar.enableCrashRecovery();

            // Return the new status
            const running = sidecar.isRunning();
            const sidecarVersion = running ? sidecar.version() : null;

            console.log(`[ENI] POST /restart — complete, sidecar_running: ${running}`);

            res.json({
                sidecar_running: running,
                sidecar_version: sidecarVersion,
                plugin_version: getPluginVersion(),
            });
        } catch (err) {
            console.error(`[ENI] POST /restart — error: ${err.message}`);
            res.status(500).json({
                error: 'Failed to restart sidecar',
                message: err.message,
            });
        }
    });
}

module.exports = { registerRoutes };
