/**
 * Unit tests for lib/binary.js — platform detection and path utilities
 *
 * Tests getBinaryName() and getBinaryPath() across all supported platforms
 * and verifies error handling for unsupported platforms.
 */

'use strict';

const assert = require('assert');
const path = require('path');

// We need to test with different platform/arch values, so we'll mock process properties
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

console.log('lib/binary.js — getBinaryName()');

test('returns correct name for darwin-arm64', () => {
    setPlatform('darwin', 'arm64');
    const binary = freshRequire();
    assert.strictEqual(binary.getBinaryName(), 'eni-sidecar-darwin-arm64');
});

test('returns correct name for darwin-x64', () => {
    setPlatform('darwin', 'x64');
    const binary = freshRequire();
    assert.strictEqual(binary.getBinaryName(), 'eni-sidecar-darwin-x64');
});

test('returns correct name for linux-x64', () => {
    setPlatform('linux', 'x64');
    const binary = freshRequire();
    assert.strictEqual(binary.getBinaryName(), 'eni-sidecar-linux-x64');
});

test('returns correct name for win32-x64 with .exe extension', () => {
    setPlatform('win32', 'x64');
    const binary = freshRequire();
    assert.strictEqual(binary.getBinaryName(), 'eni-sidecar-win32-x64.exe');
});

test('returns null for unsupported platform', () => {
    setPlatform('freebsd', 'x64');
    // Suppress console.error output during test
    const origError = console.error;
    let errorMsg = '';
    console.error = (msg) => { errorMsg = msg; };
    const binary = freshRequire();
    const result = binary.getBinaryName();
    console.error = origError;
    assert.strictEqual(result, null);
    assert.ok(errorMsg.includes('[ENI]'), 'Error message should have [ENI] prefix');
    assert.ok(errorMsg.includes('freebsd-x64'), 'Error message should include the unsupported platform');
});

test('returns null for unsupported arch', () => {
    setPlatform('linux', 'arm64');
    const origError = console.error;
    let errorMsg = '';
    console.error = (msg) => { errorMsg = msg; };
    const binary = freshRequire();
    const result = binary.getBinaryName();
    console.error = origError;
    assert.strictEqual(result, null);
});

console.log('\nlib/binary.js — getBinaryPath()');

test('returns full path using path.join for darwin-arm64', () => {
    setPlatform('darwin', 'arm64');
    const binary = freshRequire();
    const expected = path.join(__dirname, '..', 'bin', 'eni-sidecar-darwin-arm64');
    assert.strictEqual(binary.getBinaryPath(), expected);
});

test('returns full path with .exe for win32-x64', () => {
    setPlatform('win32', 'x64');
    const binary = freshRequire();
    const expected = path.join(__dirname, '..', 'bin', 'eni-sidecar-win32-x64.exe');
    assert.strictEqual(binary.getBinaryPath(), expected);
});

test('returns null when platform is unsupported', () => {
    setPlatform('freebsd', 'x64');
    const origError = console.error;
    console.error = () => {};
    const binary = freshRequire();
    const result = binary.getBinaryPath();
    console.error = origError;
    assert.strictEqual(result, null);
});

console.log('\nlib/binary.js — SUPPORTED_PLATFORMS');

test('contains exactly 4 supported platforms', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.SUPPORTED_PLATFORMS.size, 4);
    assert.ok(binary.SUPPORTED_PLATFORMS.has('darwin-arm64'));
    assert.ok(binary.SUPPORTED_PLATFORMS.has('darwin-x64'));
    assert.ok(binary.SUPPORTED_PLATFORMS.has('linux-x64'));
    assert.ok(binary.SUPPORTED_PLATFORMS.has('win32-x64'));
});

console.log('\nlib/binary.js — compareVersions()');

test('returns 0 for equal versions', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.2.3', '1.2.3'), 0);
});

test('returns -1 when a < b (major)', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.0.0', '2.0.0'), -1);
});

test('returns 1 when a > b (major)', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('3.0.0', '2.0.0'), 1);
});

test('returns -1 when a < b (minor)', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.2.0', '1.3.0'), -1);
});

test('returns 1 when a > b (minor)', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.5.0', '1.3.0'), 1);
});

test('returns -1 when a < b (patch)', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.2.3', '1.2.4'), -1);
});

test('returns 1 when a > b (patch)', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.2.5', '1.2.4'), 1);
});

test('returns 0 for invalid version string a', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('invalid', '1.2.3'), 0);
});

test('returns 0 for invalid version string b', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.2.3', 'bad'), 0);
});

test('returns 0 when both versions are invalid', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('abc', 'xyz'), 0);
});

test('returns 0 for partial version strings', () => {
    const binary = freshRequire();
    assert.strictEqual(binary.compareVersions('1.2', '1.2.3'), 0);
});

test('handles numeric comparison correctly (not string comparison)', () => {
    const binary = freshRequire();
    // String comparison would say "9" > "10", numeric says 9 < 10
    assert.strictEqual(binary.compareVersions('0.9.0', '0.10.0'), -1);
});

console.log('\nlib/binary.js — getLocalVersion()');

test('returns null when binary does not exist', () => {
    setPlatform('darwin', 'arm64');
    const binary = freshRequire();
    // Binary won't exist in bin/ during tests
    assert.strictEqual(binary.getLocalVersion(), null);
});

test('returns null when platform is unsupported', () => {
    setPlatform('freebsd', 'x64');
    const origError = console.error;
    console.error = () => {};
    const binary = freshRequire();
    const result = binary.getLocalVersion();
    console.error = origError;
    assert.strictEqual(result, null);
});

// Summary
console.log(`\n${passed + failed} tests: ${passed} passed, ${failed} failed`);
if (failed > 0) {
    process.exit(1);
}
