/**
 * Property-Based Test: Config Merge Idempotence (Property 4)
 *
 * Validates: Requirements 6.1, 6.2, 6.3
 *
 * Property: For any partial config object (including empty), merging with
 * defaults produces an object with all required keys and valid types.
 *
 * Run with: node lib/config.test.js
 */

'use strict';

const assert = require('assert');
const fs = require('fs');
const path = require('path');

const CONFIG_PATH = path.join(__dirname, '..', 'config.json');
const CONFIG_MODULE_PATH = path.resolve(__dirname, 'config.js');

// Save original config.json if it exists, restore after tests
let originalConfig = null;
let originalConfigExists = false;

function setup() {
    if (fs.existsSync(CONFIG_PATH)) {
        originalConfig = fs.readFileSync(CONFIG_PATH, 'utf-8');
        originalConfigExists = true;
    }
}

function teardown() {
    if (originalConfigExists) {
        fs.writeFileSync(CONFIG_PATH, originalConfig, 'utf-8');
    } else {
        try { fs.unlinkSync(CONFIG_PATH); } catch (e) { /* ignore */ }
    }
}

/**
 * Load config module fresh by clearing the require cache.
 */
function loadConfigFresh() {
    delete require.cache[CONFIG_MODULE_PATH];
    return require(CONFIG_MODULE_PATH);
}

/**
 * Write a config object to config.json and load the config module fresh.
 */
function writeAndLoad(configObj) {
    fs.writeFileSync(CONFIG_PATH, JSON.stringify(configObj), 'utf-8');
    return loadConfigFresh();
}

// --- Random generators ---

function randomInt(min, max) {
    return Math.floor(Math.random() * (max - min + 1)) + min;
}

function randomFloat() {
    return (Math.random() - 0.5) * 20000;
}

function randomString() {
    const chars = 'abcdefghijklmnopqrstuvwxyz/._-0123456789';
    const len = randomInt(0, 30);
    let s = '';
    for (let i = 0; i < len; i++) {
        s += chars[Math.floor(Math.random() * chars.length)];
    }
    return s;
}

function randomValue() {
    const type = randomInt(0, 6);
    switch (type) {
        case 0: return randomInt(-1000, 65535);
        case 1: return Math.random() > 0.5;
        case 2: return randomString();
        case 3: return null;
        case 4: return undefined;
        case 5: return randomFloat();
        case 6: return [1, 2, 3];
        default: return null;
    }
}

/**
 * Generate a random valid value for sidecarPort.
 */
function randomPort() {
    return randomInt(1, 65535);
}

/**
 * Generate a random valid value for autoUpdate.
 */
function randomBoolean() {
    return Math.random() > 0.5;
}

/**
 * Generate a random valid value for dataRoot.
 */
function randomDataRoot() {
    return Math.random() > 0.5 ? randomString() : null;
}

/**
 * Generate a random partial config object with valid-typed values.
 * Randomly includes/excludes known keys and may add unknown keys.
 */
function generatePartialConfig() {
    const config = {};

    // Randomly include sidecarPort (valid number > 0)
    if (Math.random() > 0.4) {
        config.sidecarPort = randomPort();
    }

    // Randomly include autoUpdate (valid boolean)
    if (Math.random() > 0.4) {
        config.autoUpdate = randomBoolean();
    }

    // Randomly include dataRoot (valid string or null)
    if (Math.random() > 0.4) {
        config.dataRoot = randomDataRoot();
    }

    // Randomly add unknown keys (any type — these should not break the merge)
    const extraKeys = randomInt(0, 3);
    for (let i = 0; i < extraKeys; i++) {
        const key = 'extra_' + randomString().slice(0, 8);
        config[key] = randomValue();
    }

    return config;
}

// --- Assertions ---

/**
 * Verify the config object has all required keys with valid types.
 */
function assertValidConfig(config, label) {
    // sidecarPort key must exist
    assert.ok('sidecarPort' in config, `${label}: missing sidecarPort key`);
    // autoUpdate key must exist
    assert.ok('autoUpdate' in config, `${label}: missing autoUpdate key`);
    // dataRoot key must exist
    assert.ok('dataRoot' in config, `${label}: missing dataRoot key`);

    // Must have sidecarPort as a number > 0
    assert.strictEqual(typeof config.sidecarPort, 'number',
        `${label}: sidecarPort should be a number, got ${typeof config.sidecarPort} (${config.sidecarPort})`);
    assert.ok(config.sidecarPort > 0,
        `${label}: sidecarPort should be > 0, got ${config.sidecarPort}`);

    // Must have autoUpdate as a boolean
    assert.strictEqual(typeof config.autoUpdate, 'boolean',
        `${label}: autoUpdate should be a boolean, got ${typeof config.autoUpdate} (${config.autoUpdate})`);

    // Must have dataRoot as string or null
    assert.ok(
        config.dataRoot === null || typeof config.dataRoot === 'string',
        `${label}: dataRoot should be string or null, got ${typeof config.dataRoot} (${config.dataRoot})`
    );
}

// --- Test Cases ---

let passed = 0;
let failed = 0;

function runTest(name, fn) {
    try {
        fn();
        passed++;
        console.log(`  ✓ ${name}`);
    } catch (err) {
        failed++;
        console.log(`  ✗ ${name}`);
        console.log(`    ${err.message}`);
    }
}

// ============================================================
console.log('Property 4: Config Merge Idempotence');
console.log('Validates: Requirements 6.1, 6.2, 6.3');
console.log('');

setup();

try {
    // --- Property test: random partial configs always produce valid output ---
    console.log('Property test: random partial configs (100 iterations)');

    const ITERATIONS = 100;

    runTest(`All ${ITERATIONS} random partial configs produce valid merged config`, () => {
        for (let i = 0; i < ITERATIONS; i++) {
            const partial = generatePartialConfig();
            // Remove undefined values before JSON serialization (JSON.stringify drops them)
            const serializable = JSON.parse(JSON.stringify(partial));
            const result = writeAndLoad(serializable);
            const label = `iteration ${i} with input ${JSON.stringify(serializable)}`;

            assertValidConfig(result, label);

            // Verify user-provided values override defaults
            if ('sidecarPort' in serializable) {
                assert.strictEqual(result.sidecarPort, serializable.sidecarPort,
                    `${label}: sidecarPort should be overridden by user value`);
            } else {
                assert.strictEqual(result.sidecarPort, 7842,
                    `${label}: sidecarPort should fall back to default 7842`);
            }

            if ('autoUpdate' in serializable) {
                assert.strictEqual(result.autoUpdate, serializable.autoUpdate,
                    `${label}: autoUpdate should be overridden by user value`);
            } else {
                assert.strictEqual(result.autoUpdate, true,
                    `${label}: autoUpdate should fall back to default true`);
            }

            if ('dataRoot' in serializable) {
                assert.strictEqual(result.dataRoot, serializable.dataRoot,
                    `${label}: dataRoot should be overridden by user value`);
            } else {
                assert.strictEqual(result.dataRoot, null,
                    `${label}: dataRoot should fall back to default null`);
            }
        }
    });

    // --- Property: user-provided valid values override defaults ---
    console.log('');
    console.log('Property: user-provided values override defaults');

    runTest('User sidecarPort overrides default', () => {
        const result = writeAndLoad({ sidecarPort: 9999 });
        assert.strictEqual(result.sidecarPort, 9999);
    });

    runTest('User autoUpdate=false overrides default', () => {
        const result = writeAndLoad({ autoUpdate: false });
        assert.strictEqual(result.autoUpdate, false);
    });

    runTest('User dataRoot string overrides default null', () => {
        const result = writeAndLoad({ dataRoot: '/custom/path' });
        assert.strictEqual(result.dataRoot, '/custom/path');
    });

    // --- Property: missing keys get default values ---
    console.log('');
    console.log('Property: missing keys get default values');

    runTest('Empty object gets all defaults', () => {
        const result = writeAndLoad({});
        assert.strictEqual(result.sidecarPort, 7842);
        assert.strictEqual(result.autoUpdate, true);
        assert.strictEqual(result.dataRoot, null);
    });

    runTest('Partial object (only sidecarPort) gets other defaults', () => {
        const result = writeAndLoad({ sidecarPort: 5000 });
        assert.strictEqual(result.sidecarPort, 5000);
        assert.strictEqual(result.autoUpdate, true);
        assert.strictEqual(result.dataRoot, null);
    });

    runTest('Partial object (only autoUpdate) gets other defaults', () => {
        const result = writeAndLoad({ autoUpdate: false });
        assert.strictEqual(result.sidecarPort, 7842);
        assert.strictEqual(result.autoUpdate, false);
        assert.strictEqual(result.dataRoot, null);
    });

    // --- Edge cases ---
    console.log('');
    console.log('Edge cases');

    runTest('Empty object {} produces valid config', () => {
        const result = writeAndLoad({});
        assertValidConfig(result, 'empty object');
    });

    runTest('Null values in config are preserved (dataRoot: null)', () => {
        const result = writeAndLoad({ dataRoot: null });
        assert.strictEqual(result.dataRoot, null);
        assertValidConfig(result, 'null dataRoot');
    });

    runTest('Extra unknown keys do not break config', () => {
        const result = writeAndLoad({
            sidecarPort: 8080,
            unknownKey: 'hello',
            anotherExtra: 42
        });
        assertValidConfig(result, 'extra keys');
        assert.strictEqual(result.sidecarPort, 8080);
    });

    runTest('Invalid type for sidecarPort (string) still produces a config with all keys', () => {
        const result = writeAndLoad({ sidecarPort: 'not-a-number' });
        // The merge uses spread, so user value overrides — config has all keys
        assert.ok('sidecarPort' in result, 'sidecarPort key must exist');
        assert.ok('autoUpdate' in result, 'autoUpdate key must exist');
        assert.ok('dataRoot' in result, 'dataRoot key must exist');
    });

    runTest('Invalid type for autoUpdate (number) still produces a config with all keys', () => {
        const result = writeAndLoad({ autoUpdate: 42 });
        assert.ok('sidecarPort' in result, 'sidecarPort key must exist');
        assert.ok('autoUpdate' in result, 'autoUpdate key must exist');
        assert.ok('dataRoot' in result, 'dataRoot key must exist');
    });

    runTest('Malformed JSON in config.json falls back to all defaults', () => {
        fs.writeFileSync(CONFIG_PATH, '{invalid json!!!', 'utf-8');
        const result = loadConfigFresh();
        assert.strictEqual(result.sidecarPort, 7842);
        assert.strictEqual(result.autoUpdate, true);
        assert.strictEqual(result.dataRoot, null);
    });

    runTest('Missing config.json falls back to all defaults', () => {
        try { fs.unlinkSync(CONFIG_PATH); } catch (e) { /* ignore */ }
        const result = loadConfigFresh();
        assert.strictEqual(result.sidecarPort, 7842);
        assert.strictEqual(result.autoUpdate, true);
        assert.strictEqual(result.dataRoot, null);
    });

} finally {
    teardown();
}

// --- Summary ---
console.log('');
console.log(`Results: ${passed} passed, ${failed} failed, ${passed + failed} total`);

if (failed > 0) {
    process.exit(1);
}

console.log('');
console.log('All property tests passed ✓');
