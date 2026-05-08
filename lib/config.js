/**
 * ENI World Builder — Configuration Manager
 *
 * Loads config.json from the plugin directory, merges with defaults,
 * and exports a frozen config object.
 *
 * Requirements: 6.1, 6.2, 6.3, 6.5
 */

'use strict';

const path = require('path');
const fs = require('fs');

/** Default configuration values */
const DEFAULTS = {
    sidecarPort: 7842,
    autoUpdate: true,
    dataRoot: null,
};

/**
 * Load and merge configuration from config.json.
 * Falls back to defaults if the file is missing or contains malformed JSON.
 *
 * @returns {Readonly<{sidecarPort: number, autoUpdate: boolean, dataRoot: string|null}>}
 */
function loadConfig() {
    const configPath = path.join(__dirname, '..', 'config.json');

    let userConfig = {};

    try {
        const raw = fs.readFileSync(configPath, 'utf-8');
        userConfig = JSON.parse(raw);
    } catch (err) {
        // Missing file (ENOENT) or malformed JSON — fall back to defaults silently
        if (err.code !== 'ENOENT') {
            console.log('[ENI] config.json is malformed, using defaults');
        }
    }

    // Merge: user values override defaults
    const merged = { ...DEFAULTS, ...userConfig };

    return Object.freeze(merged);
}

module.exports = loadConfig();
