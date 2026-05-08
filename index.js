/**
 * ENI World Builder — Server Plugin Entry Point
 *
 * Exports init(router) and exit() per the SillyTavern server plugin contract.
 * Orchestrates initialization: config → platform check → download → spawn → install UI → register routes.
 * Wraps all logic in try/catch to never crash SillyTavern.
 *
 * Requirements: 1.1, 1.3, 6.4, 6.5
 */

'use strict';

const config = require('./lib/config');
const { getBinaryName, checkForUpdate } = require('./lib/binary');
const sidecar = require('./lib/sidecar');
const uiInstaller = require('./lib/ui-installer');
const registerRoutes = require('./lib/routes');

/**
 * Initialize the ENI World Builder plugin.
 *
 * Orchestrates the full startup sequence:
 *   1. Load config (done via require)
 *   2. Check platform support
 *   3. Binary management (download/update)
 *   4. Sidecar lifecycle (spawn + crash recovery)
 *   5. UI extension installation
 *   6. Register API routes
 *
 * Never throws — all errors are caught and logged at error level.
 *
 * @param {import('express').Router} router - Express router for registering plugin routes
 */
async function init(router) {
    try {
        console.log('[ENI] Initializing ENI World Builder plugin...');

        // 1. Config is already loaded via require('./lib/config')

        // 2. Check platform support
        const binaryName = getBinaryName();
        if (!binaryName) {
            console.warn('[ENI] Platform not supported — running in degraded mode (no sidecar)');
        }

        // 3. Binary management — download or update
        if (binaryName) {
            try {
                await checkForUpdate();
            } catch (err) {
                console.error(`[ENI] Binary update check failed: ${err.message}`);
            }
        }

        // 4. Sidecar lifecycle — spawn and enable crash recovery
        if (binaryName) {
            try {
                const spawned = await sidecar.spawn();
                if (spawned) {
                    sidecar.enableCrashRecovery();
                }
            } catch (err) {
                console.error(`[ENI] Sidecar spawn failed: ${err.message}`);
            }
        }

        // 5. UI extension installation
        try {
            uiInstaller.install();
        } catch (err) {
            console.error(`[ENI] UI extension installation failed: ${err.message}`);
        }

        // 6. Register API routes
        registerRoutes(router);

        console.log('[ENI] Plugin initialized successfully');
    } catch (err) {
        console.error(`[ENI] Plugin initialization error: ${err.message}`);
    }
}

/**
 * Shut down the ENI World Builder plugin.
 *
 * Gracefully terminates the sidecar process.
 * Never throws — all errors are caught and logged at error level.
 */
async function exit() {
    try {
        console.log('[ENI] Shutting down ENI World Builder plugin...');
        await sidecar.kill();
        console.log('[ENI] Plugin shutdown complete');
    } catch (err) {
        console.error(`[ENI] Plugin shutdown error: ${err.message}`);
    }
}

module.exports = { init, exit };
