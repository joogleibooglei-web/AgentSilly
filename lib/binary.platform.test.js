/**
 * Property Test: Platform Identifier Mapping Completeness (Property 2)
 *
 * For all supported (platform, arch) pairs, getBinaryName() returns a non-empty
 * string matching the pattern `eni-sidecar-{platform}-{arch}[.exe]`.
 *
 * This is an example-based test over the finite set of 4 supported platforms.
 *
 * **Validates: Requirements 2.3, 7.4**
 */

'use strict';

const assert = require('assert');

// Save original process properties for restoration
const originalPlatform = Object.getOwnPropertyDescriptor(process, 'platform');
const originalArch = Object.getOwnPropertyDescriptor(process, 'arch');

function setPlatform(platform, arch) {
    Object.defineProperty(process, 'platform', { value: platform, writable: true });
    Object.defineProperty(process, 'arch', { value: arch, writable: true });
}

function restorePlatform() {
    Object.defineProperty(process, 'platform', originalPlatform);
    Object.defineProperty(process, 'arch', originalArch);
}

/**
 * Re-require the module to pick up new process.platform/arch values.
 */
function freshRequire() {
    delete require.cache[require.resolve('./binary.js')];
    return require('./binary.js');
}

let passed = 0;
let failed = 0;

function test(name, fn) {
    try {
        fn();
        passed++;
        console.log(`  ✓ ${name}`);
    } catch (err) {
        failed++;
        console.error(`  ✗ ${name}`);
        console.error(`    ${err.message}`);
    } finally {
        restorePlatform();
    }
}

// Complete set of supported platforms
const SUPPORTED_PAIRS = [
    { platform: 'darwin', arch: 'arm64' },
    { platform: 'darwin', arch: 'x64' },
    { platform: 'linux', arch: 'x64' },
    { platform: 'win32', arch: 'x64' },
];

console.log('Property 2: Platform Identifier Mapping Completeness');
console.log('====================================================\n');

console.log('For each supported (platform, arch) pair:');

for (const { platform, arch } of SUPPORTED_PAIRS) {
    const identifier = `${platform}-${arch}`;

    test(`${identifier}: getBinaryName() returns a non-empty string`, () => {
        setPlatform(platform, arch);
        const binary = freshRequire();
        const result = binary.getBinaryName();
        assert.ok(result !== null, `Expected non-null result for ${identifier}`);
        assert.ok(typeof result === 'string', `Expected string result for ${identifier}`);
        assert.ok(result.length > 0, `Expected non-empty string for ${identifier}`);
    });

    test(`${identifier}: result matches pattern eni-sidecar-{platform}-{arch}[.exe]`, () => {
        setPlatform(platform, arch);
        const binary = freshRequire();
        const result = binary.getBinaryName();

        const expectedBase = `eni-sidecar-${platform}-${arch}`;
        const expectedFull = platform === 'win32' ? `${expectedBase}.exe` : expectedBase;

        assert.strictEqual(result, expectedFull,
            `Expected "${expectedFull}" but got "${result}"`);
    });

    test(`${identifier}: result starts with "eni-sidecar-"`, () => {
        setPlatform(platform, arch);
        const binary = freshRequire();
        const result = binary.getBinaryName();
        assert.ok(result.startsWith('eni-sidecar-'),
            `Expected result to start with "eni-sidecar-" but got "${result}"`);
    });

    test(`${identifier}: .exe appended only for win32`, () => {
        setPlatform(platform, arch);
        const binary = freshRequire();
        const result = binary.getBinaryName();

        if (platform === 'win32') {
            assert.ok(result.endsWith('.exe'),
                `Expected .exe extension for win32 but got "${result}"`);
        } else {
            assert.ok(!result.endsWith('.exe'),
                `Expected no .exe extension for ${platform} but got "${result}"`);
        }
    });
}

console.log('\nUnsupported platforms return null:');

const UNSUPPORTED_PAIRS = [
    { platform: 'freebsd', arch: 'x64' },
    { platform: 'linux', arch: 'arm64' },
    { platform: 'win32', arch: 'arm64' },
    { platform: 'sunos', arch: 'x64' },
    { platform: 'aix', arch: 'ppc64' },
];

for (const { platform, arch } of UNSUPPORTED_PAIRS) {
    const identifier = `${platform}-${arch}`;

    test(`${identifier}: getBinaryName() returns null`, () => {
        setPlatform(platform, arch);
        // Suppress console.error output during test
        const origError = console.error;
        console.error = () => {};
        const binary = freshRequire();
        const result = binary.getBinaryName();
        console.error = origError;
        assert.strictEqual(result, null,
            `Expected null for unsupported platform ${identifier} but got "${result}"`);
    });
}

// Summary
console.log(`\n${passed + failed} tests: ${passed} passed, ${failed} failed`);
if (failed > 0) {
    process.exit(1);
}
