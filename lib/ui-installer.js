/**
 * ENI World Builder — UI Extension Installer
 *
 * Copies manifest.json and dist/ into the SillyTavern extensions directory
 * so the UI extension is available without a separate install step.
 *
 * Requirements: 4.1, 4.2, 4.3, 4.4, 4.5
 */

'use strict';

const path = require('path');
const fs = require('fs');
const config = require('./config');

/**
 * Resolve the SillyTavern data root directory.
 *
 * Priority:
 *   1. config.dataRoot (user override)
 *   2. global.DATA_ROOT (some ST versions expose this)
 *   3. Fall back to ../../../../data/default/ relative to this file
 *      (plugins/eni-world-builder/lib/ → ST root → data/default/)
 *
 * @returns {string} Resolved absolute path to the ST data root
 */
function getDataRoot() {
    if (config.dataRoot) {
        return path.resolve(config.dataRoot);
    }

    if (global.DATA_ROOT) {
        return path.resolve(global.DATA_ROOT);
    }

    // __dirname = plugins/eni-world-builder/lib/
    // Go up to ST root (../../..) then into data/default/
    return path.resolve(__dirname, '..', '..', '..', '..', 'data', 'default');
}

/**
 * Get the target directory where UI extension files are installed.
 *
 * SillyTavern loads frontend extensions from public/scripts/extensions/third-party/.
 *
 * @returns {string} Absolute path to the extensions/third-party/eni-world-builder/ directory
 */
function getTargetDir() {
    // __dirname = plugins/eni-world-builder/lib/
    // ST root = ../../..
    const stRoot = path.resolve(__dirname, '..', '..', '..');
    return path.join(stRoot, 'public', 'scripts', 'extensions', 'third-party', 'eni-world-builder');
}

/**
 * Check whether the UI extension needs to be installed or updated.
 *
 * Compares the version field in the installed manifest.json against
 * the plugin's source manifest.json.
 *
 * @returns {boolean} true if versions differ or installed manifest doesn't exist
 */
function needsUpdate() {
    const targetDir = getTargetDir();
    const installedManifestPath = path.join(targetDir, 'manifest.json');
    const pluginManifestPath = path.join(__dirname, '..', 'manifest.json');

    try {
        const installedRaw = fs.readFileSync(installedManifestPath, 'utf-8');
        const installedManifest = JSON.parse(installedRaw);

        const pluginRaw = fs.readFileSync(pluginManifestPath, 'utf-8');
        const pluginManifest = JSON.parse(pluginRaw);

        return installedManifest.version !== pluginManifest.version;
    } catch (err) {
        // If installed manifest doesn't exist or is unreadable, update is needed
        return true;
    }
}

/**
 * Recursively copy a directory from src to dest.
 *
 * @param {string} src - Source directory path
 * @param {string} dest - Destination directory path
 */
function copyDirSync(src, dest) {
    fs.mkdirSync(dest, { recursive: true });

    const entries = fs.readdirSync(src, { withFileTypes: true });

    for (const entry of entries) {
        const srcPath = path.join(src, entry.name);
        const destPath = path.join(dest, entry.name);

        if (entry.isDirectory()) {
            copyDirSync(srcPath, destPath);
        } else {
            fs.copyFileSync(srcPath, destPath);
        }
    }
}

/**
 * Install the UI extension files into the SillyTavern extensions directory.
 *
 * Copies manifest.json and dist/ from the plugin root to the target directory.
 * Skips the operation if the installed version already matches.
 * Logs a descriptive error on permissions failure and continues.
 */
function install() {
    if (!needsUpdate()) {
        console.log('[ENI] UI extension is up to date, skipping install');
        return;
    }

    const targetDir = getTargetDir();
    const pluginRoot = path.join(__dirname, '..');
    const manifestSrc = path.join(pluginRoot, 'manifest.json');
    const distSrc = path.join(pluginRoot, 'dist');

    try {
        // Create target directory recursively if it doesn't exist
        fs.mkdirSync(targetDir, { recursive: true });

        // Copy manifest.json
        fs.copyFileSync(manifestSrc, path.join(targetDir, 'manifest.json'));

        // Copy dist/ directory recursively
        const distDest = path.join(targetDir, 'dist');
        copyDirSync(distSrc, distDest);

        console.log(`[ENI] UI extension installed to ${targetDir}`);
    } catch (err) {
        if (err.code === 'EACCES' || err.code === 'EPERM') {
            console.error(`[ENI] Permission denied while installing UI extension to ${targetDir}. Please check directory permissions.`);
        } else {
            throw err;
        }
    }
}

module.exports = {
    getDataRoot,
    getTargetDir,
    needsUpdate,
    install,
    copyDirSync,
};
