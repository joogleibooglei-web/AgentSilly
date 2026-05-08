/**
 * Unit tests for lib/sidecar.js — spawn and health check
 *
 * Tests isAlreadyRunning(), waitForHealth(), and spawn() behavior
 * using a mock HTTP server and process mocking.
 */

'use strict';

const assert = require('assert');
const http = require('http');

let passed = 0;
let failed = 0;

function test(name, fn) {
    return fn()
        .then(() => {
            passed++;
            console.log(`  ✓ ${name}`);
        })
        .catch((err) => {
            failed++;
            console.error(`  ✗ ${name}`);
            console.error(`    ${err.message}`);
        });
}

/**
 * Re-require the sidecar module to get a fresh instance.
 */
function freshRequire() {
    delete require.cache[require.resolve('./sidecar.js')];
    return require('./sidecar.js');
}

async function runTests() {
    console.log('lib/sidecar.js — isAlreadyRunning()');

    await test('returns false when no server is running on the port', async () => {
        const sidecar = freshRequire();
        // Port 19999 should not have anything running
        const origPort = require('./config').sidecarPort;
        // We test with the default config port — nothing should be running there in test
        const result = await sidecar.isAlreadyRunning();
        assert.strictEqual(result, false);
    });

    await test('returns true when a server responds with 200 on the health endpoint', async () => {
        // Start a mock server on a random port
        const server = http.createServer((req, res) => {
            if (req.url === '/health') {
                res.writeHead(200);
                res.end('OK');
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
        const port = server.address().port;

        // Override config to use our test port
        delete require.cache[require.resolve('./config')];
        delete require.cache[require.resolve('./sidecar.js')];
        const configPath = require.resolve('./config');
        require.cache[configPath] = {
            id: configPath,
            filename: configPath,
            loaded: true,
            exports: Object.freeze({ sidecarPort: port, autoUpdate: true, dataRoot: null }),
        };

        const sidecar = require('./sidecar.js');
        const result = await sidecar.isAlreadyRunning();
        assert.strictEqual(result, true);

        await new Promise((resolve) => server.close(resolve));

        // Restore config
        delete require.cache[configPath];
        delete require.cache[require.resolve('./sidecar.js')];
    });

    console.log('\nlib/sidecar.js — waitForHealth()');

    await test('returns true when health endpoint responds with 200', async () => {
        const server = http.createServer((req, res) => {
            if (req.url === '/health') {
                res.writeHead(200);
                res.end('OK');
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
        const port = server.address().port;

        // Override config
        delete require.cache[require.resolve('./config')];
        delete require.cache[require.resolve('./sidecar.js')];
        const configPath = require.resolve('./config');
        require.cache[configPath] = {
            id: configPath,
            filename: configPath,
            loaded: true,
            exports: Object.freeze({ sidecarPort: port, autoUpdate: true, dataRoot: null }),
        };

        const sidecar = require('./sidecar.js');
        const result = await sidecar.waitForHealth();
        assert.strictEqual(result, true);

        await new Promise((resolve) => server.close(resolve));

        // Restore
        delete require.cache[configPath];
        delete require.cache[require.resolve('./sidecar.js')];
    });

    await test('returns true when health endpoint becomes available after a delay', async () => {
        let requestCount = 0;
        const server = http.createServer((req, res) => {
            requestCount++;
            if (req.url === '/health' && requestCount >= 3) {
                res.writeHead(200);
                res.end('OK');
            } else {
                res.writeHead(503);
                res.end();
            }
        });

        await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
        const port = server.address().port;

        // Override config
        delete require.cache[require.resolve('./config')];
        delete require.cache[require.resolve('./sidecar.js')];
        const configPath = require.resolve('./config');
        require.cache[configPath] = {
            id: configPath,
            filename: configPath,
            loaded: true,
            exports: Object.freeze({ sidecarPort: port, autoUpdate: true, dataRoot: null }),
        };

        const sidecar = require('./sidecar.js');
        const result = await sidecar.waitForHealth();
        assert.strictEqual(result, true);
        assert.ok(requestCount >= 3, `Expected at least 3 requests, got ${requestCount}`);

        await new Promise((resolve) => server.close(resolve));

        // Restore
        delete require.cache[configPath];
        delete require.cache[require.resolve('./sidecar.js')];
    });

    console.log('\nlib/sidecar.js — spawn()');

    await test('returns false when binary path is null (unsupported platform)', async () => {
        // Mock binary module to return null
        delete require.cache[require.resolve('./sidecar.js')];
        delete require.cache[require.resolve('./binary.js')];
        const binaryPath = require.resolve('./binary.js');
        require.cache[binaryPath] = {
            id: binaryPath,
            filename: binaryPath,
            loaded: true,
            exports: {
                getBinaryPath: () => null,
                getBinaryName: () => null,
            },
        };

        const origError = console.error;
        let errorMsg = '';
        console.error = (msg) => { errorMsg = msg; };

        const sidecar = require('./sidecar.js');
        const result = await sidecar.spawn();

        console.error = origError;
        assert.strictEqual(result, false);
        assert.ok(errorMsg.includes('[ENI]'), 'Error should have [ENI] prefix');

        // Restore
        delete require.cache[binaryPath];
        delete require.cache[require.resolve('./sidecar.js')];
    });

    await test('returns false when binary file does not exist', async () => {
        // Mock binary module to return a non-existent path
        delete require.cache[require.resolve('./sidecar.js')];
        delete require.cache[require.resolve('./binary.js')];
        const binaryPath = require.resolve('./binary.js');
        require.cache[binaryPath] = {
            id: binaryPath,
            filename: binaryPath,
            loaded: true,
            exports: {
                getBinaryPath: () => '/tmp/nonexistent-binary-path-12345',
                getBinaryName: () => 'nonexistent',
            },
        };

        const origError = console.error;
        let errorMsg = '';
        console.error = (msg) => { errorMsg = msg; };

        const sidecar = require('./sidecar.js');
        const result = await sidecar.spawn();

        console.error = origError;
        assert.strictEqual(result, false);
        assert.ok(errorMsg.includes('[ENI]'), 'Error should have [ENI] prefix');
        assert.ok(errorMsg.includes('not found'), 'Error should mention binary not found');

        // Restore
        delete require.cache[binaryPath];
        delete require.cache[require.resolve('./sidecar.js')];
    });

    await test('returns true when sidecar is already running (skips spawn)', async () => {
        // Start a mock health server
        const server = http.createServer((req, res) => {
            if (req.url === '/health') {
                res.writeHead(200);
                res.end('OK');
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
        const port = server.address().port;

        // Override config and binary
        delete require.cache[require.resolve('./config')];
        delete require.cache[require.resolve('./binary.js')];
        delete require.cache[require.resolve('./sidecar.js')];

        const configPath = require.resolve('./config');
        require.cache[configPath] = {
            id: configPath,
            filename: configPath,
            loaded: true,
            exports: Object.freeze({ sidecarPort: port, autoUpdate: true, dataRoot: null }),
        };

        const binaryModPath = require.resolve('./binary.js');
        require.cache[binaryModPath] = {
            id: binaryModPath,
            filename: binaryModPath,
            loaded: true,
            exports: {
                getBinaryPath: () => '/some/path/that/exists',
                getBinaryName: () => 'eni-sidecar-test',
            },
        };

        // Mock fs.existsSync to return true for the binary path
        const fs = require('fs');
        const origExistsSync = fs.existsSync;
        fs.existsSync = (p) => {
            if (p === '/some/path/that/exists') return true;
            return origExistsSync(p);
        };

        const origLog = console.log;
        let logMsg = '';
        console.log = (msg) => { logMsg = msg; };

        const sidecar = require('./sidecar.js');
        const result = await sidecar.spawn();

        console.log = origLog;
        fs.existsSync = origExistsSync;

        assert.strictEqual(result, true);
        assert.ok(logMsg.includes('Existing sidecar instance found'), `Expected log about existing instance, got: ${logMsg}`);

        await new Promise((resolve) => server.close(resolve));

        // Restore
        delete require.cache[configPath];
        delete require.cache[binaryModPath];
        delete require.cache[require.resolve('./sidecar.js')];
    });

    console.log('\nlib/sidecar.js — module exports');

    await test('exports spawn, waitForHealth, isAlreadyRunning, getProcess, setProcess', async () => {
        const sidecar = freshRequire();
        assert.strictEqual(typeof sidecar.spawn, 'function');
        assert.strictEqual(typeof sidecar.waitForHealth, 'function');
        assert.strictEqual(typeof sidecar.isAlreadyRunning, 'function');
        assert.strictEqual(typeof sidecar.getProcess, 'function');
        assert.strictEqual(typeof sidecar.setProcess, 'function');
    });

    await test('getProcess returns null initially', async () => {
        const sidecar = freshRequire();
        assert.strictEqual(sidecar.getProcess(), null);
    });

    await test('setProcess updates the internal process reference', async () => {
        const sidecar = freshRequire();
        const mockProcess = { pid: 12345 };
        sidecar.setProcess(mockProcess);
        assert.strictEqual(sidecar.getProcess(), mockProcess);
        // Clean up
        sidecar.setProcess(null);
    });

    console.log('\nlib/sidecar.js — kill()');

    await test('kill() resolves immediately when no process is running', async () => {
        const sidecar = freshRequire();
        // No process set, should resolve immediately
        await sidecar.kill();
        // If we get here, it resolved
        assert.ok(true);
    });

    await test('kill() sends SIGTERM and resolves when process exits', async () => {
        const sidecar = freshRequire();
        const { spawn: cpSpawn } = require('child_process');

        // Spawn a simple long-running process to test kill
        const child = cpSpawn('sleep', ['30']);
        sidecar.setProcess(child);

        await sidecar.kill();
        assert.strictEqual(sidecar.getProcess(), null);
    });

    console.log('\nlib/sidecar.js — isRestartAllowed()');

    await test('allows restart when no previous restarts', async () => {
        const sidecar = freshRequire();
        sidecar.resetRestartTimestamps();
        assert.strictEqual(sidecar.isRestartAllowed(), true);
    });

    await test('allows restart when fewer than 5 restarts in 60s', async () => {
        const sidecar = freshRequire();
        const now = Date.now();
        sidecar.setRestartTimestamps([now - 10000, now - 20000, now - 30000, now - 40000]);
        assert.strictEqual(sidecar.isRestartAllowed(), true);
    });

    await test('suppresses restart when 5 or more restarts in 60s', async () => {
        const sidecar = freshRequire();
        const now = Date.now();
        sidecar.setRestartTimestamps([
            now - 10000,
            now - 20000,
            now - 30000,
            now - 40000,
            now - 50000,
        ]);
        assert.strictEqual(sidecar.isRestartAllowed(), false);
    });

    await test('prunes timestamps older than 60 seconds', async () => {
        const sidecar = freshRequire();
        const now = Date.now();
        sidecar.setRestartTimestamps([
            now - 70000, // older than 60s, should be pruned
            now - 80000, // older than 60s, should be pruned
            now - 90000, // older than 60s, should be pruned
            now - 100000, // older than 60s, should be pruned
            now - 110000, // older than 60s, should be pruned
        ]);
        // All timestamps are older than 60s, so after pruning, none remain
        assert.strictEqual(sidecar.isRestartAllowed(), true);
    });

    await test('mixed timestamps: prunes old, counts recent', async () => {
        const sidecar = freshRequire();
        const now = Date.now();
        sidecar.setRestartTimestamps([
            now - 70000, // pruned
            now - 80000, // pruned
            now - 10000, // kept
            now - 20000, // kept
            now - 30000, // kept
            now - 40000, // kept
            now - 50000, // kept (5 recent = suppressed)
        ]);
        assert.strictEqual(sidecar.isRestartAllowed(), false);
    });

    console.log('\nlib/sidecar.js — state properties');

    await test('isRunning() returns false when no process', async () => {
        const sidecar = freshRequire();
        assert.strictEqual(sidecar.isRunning(), false);
    });

    await test('isRunning() returns true when process is active', async () => {
        const sidecar = freshRequire();
        const { spawn: cpSpawn } = require('child_process');
        const child = cpSpawn('sleep', ['30']);
        sidecar.setProcess(child);

        assert.strictEqual(sidecar.isRunning(), true);

        // Clean up
        child.kill('SIGKILL');
        sidecar.setProcess(null);
    });

    await test('pid() returns null when no process', async () => {
        const sidecar = freshRequire();
        assert.strictEqual(sidecar.pid(), null);
    });

    await test('pid() returns process PID when running', async () => {
        const sidecar = freshRequire();
        const { spawn: cpSpawn } = require('child_process');
        const child = cpSpawn('sleep', ['30']);
        sidecar.setProcess(child);

        assert.strictEqual(typeof sidecar.pid(), 'number');
        assert.ok(sidecar.pid() > 0);

        // Clean up
        child.kill('SIGKILL');
        sidecar.setProcess(null);
    });

    await test('version() returns a string or null', async () => {
        const sidecar = freshRequire();
        const ver = sidecar.version();
        // version() calls getLocalVersion() which may return null if binary doesn't exist
        assert.ok(ver === null || typeof ver === 'string');
    });

    console.log('\nlib/sidecar.js — exports (new functions)');

    await test('exports kill, enableCrashRecovery, isRunning, version, pid', async () => {
        const sidecar = freshRequire();
        assert.strictEqual(typeof sidecar.kill, 'function');
        assert.strictEqual(typeof sidecar.enableCrashRecovery, 'function');
        assert.strictEqual(typeof sidecar.isRunning, 'function');
        assert.strictEqual(typeof sidecar.version, 'function');
        assert.strictEqual(typeof sidecar.pid, 'function');
        assert.strictEqual(typeof sidecar.isRestartAllowed, 'function');
        assert.strictEqual(typeof sidecar.resetRestartTimestamps, 'function');
    });

    // Summary
    console.log(`\n${passed + failed} tests: ${passed} passed, ${failed} failed`);
    if (failed > 0) {
        process.exit(1);
    }
}

runTests().catch((err) => {
    console.error('Test runner error:', err);
    process.exit(1);
});
