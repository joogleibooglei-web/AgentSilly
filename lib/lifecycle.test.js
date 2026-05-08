/**
 * Integration tests for init/exit lifecycle — Property 5: Graceful Degradation
 *
 * Validates: Requirements 6.5, 2.4
 *
 * Property: For any combination of failures (network down, binary missing,
 * permissions error), init() resolves without throwing.
 * Also verifies exit() properly shuts down sidecar without throwing.
 *
 * Run with: node lib/lifecycle.test.js
 */

'use strict';

const assert = require('assert');
const path = require('path');
const Module = require('module');

// ─── Test Utilities ──────────────────────────────────────────────────────────

let testCount = 0;
let passCount = 0;
const testQueue = [];

function describe(name, fn) {
    testQueue.push({ type: 'describe', name });
    fn();
}

function it(name, fn) {
    testQueue.push({ type: 'test', name, fn });
}

async function runQueue() {
    for (const entry of testQueue) {
        if (entry.type === 'describe') {
            console.log(`\n  ${entry.name}`);
        } else {
            testCount++;
            try {
                await entry.fn();
                passCount++;
                console.log(`    ✓ ${entry.name}`);
            } catch (err) {
                console.log(`    ✗ ${entry.name}`);
                console.log(`      ${err.message}`);
                if (err.stack) {
                    const lines = err.stack.split('\n').slice(1, 4);
                    for (const line of lines) {
                        console.log(`      ${line.trim()}`);
                    }
                }
            }
        }
    }
}

// ─── Mock Setup ──────────────────────────────────────────────────────────────

/**
 * Creates a mock Express router with get/post methods that record registered routes.
 */
function createMockRouter() {
    const routes = { get: {}, post: {} };
    return {
        get(path, handler) { routes.get[path] = handler; },
        post(path, handler) { routes.post[path] = handler; },
        routes,
    };
}

/**
 * Clears all cached modules related to the plugin so we get fresh instances.
 */
function clearModuleCache() {
    const pluginRoot = path.resolve(__dirname, '..');
    const keysToDelete = Object.keys(require.cache).filter(
        (key) => key.startsWith(pluginRoot) && !key.includes('node_modules') && !key.includes('lifecycle.test.js')
    );
    for (const key of keysToDelete) {
        delete require.cache[key];
    }
}

/**
 * Installs mocks into require.cache for the specified modules.
 * Must be called AFTER clearModuleCache() and BEFORE requiring index.js.
 */
function installMocks(overrides = {}) {
    const libDir = __dirname;
    const pluginRoot = path.resolve(libDir, '..');

    // Default mocks that simulate a working (but no-binary) environment
    const defaults = {
        config: {
            sidecarPort: 7842,
            autoUpdate: true,
            dataRoot: null,
        },
        binary: {
            getBinaryName: () => null,
            getBinaryPath: () => null,
            getLocalVersion: () => null,
            checkForUpdate: async () => ({ updated: false, version: null }),
            fetchLatestRelease: async () => null,
            downloadBinary: async () => false,
            compareVersions: () => 0,
            SUPPORTED_PLATFORMS: new Set(),
        },
        sidecar: {
            spawn: async () => false,
            kill: async () => {},
            isRunning: () => false,
            version: () => null,
            pid: () => null,
            enableCrashRecovery: () => {},
            isAlreadyRunning: async () => false,
            waitForHealth: async () => false,
            getProcess: () => null,
            setProcess: () => {},
            isRestartAllowed: () => true,
            resetRestartTimestamps: () => {},
            getRestartTimestamps: () => [],
            setRestartTimestamps: () => {},
        },
        uiInstaller: {
            install: () => {},
            needsUpdate: () => false,
            getDataRoot: () => '/tmp/fake-data-root',
            getTargetDir: () => '/tmp/fake-data-root/extensions/third-party/eni-world-builder',
            copyDirSync: () => {},
        },
        routes: function registerRoutes(router) {
            // No-op route registration
        },
    };

    // Merge overrides with defaults
    const mocks = {
        config: overrides.config || defaults.config,
        binary: { ...defaults.binary, ...overrides.binary },
        sidecar: { ...defaults.sidecar, ...overrides.sidecar },
        uiInstaller: { ...defaults.uiInstaller, ...overrides.uiInstaller },
        routes: overrides.routes !== undefined ? overrides.routes : defaults.routes,
    };

    // Install mocks into require.cache
    const configPath = require.resolve(path.join(libDir, 'config.js'));
    const binaryPath = require.resolve(path.join(libDir, 'binary.js'));
    const sidecarPath = require.resolve(path.join(libDir, 'sidecar.js'));
    const uiInstallerPath = require.resolve(path.join(libDir, 'ui-installer.js'));
    const routesPath = require.resolve(path.join(libDir, 'routes.js'));

    require.cache[configPath] = { id: configPath, filename: configPath, loaded: true, exports: mocks.config };
    require.cache[binaryPath] = { id: binaryPath, filename: binaryPath, loaded: true, exports: mocks.binary };
    require.cache[sidecarPath] = { id: sidecarPath, filename: sidecarPath, loaded: true, exports: mocks.sidecar };
    require.cache[uiInstallerPath] = { id: uiInstallerPath, filename: uiInstallerPath, loaded: true, exports: mocks.uiInstaller };
    require.cache[routesPath] = { id: routesPath, filename: routesPath, loaded: true, exports: mocks.routes };
}

/**
 * Loads a fresh index.js with the given mock overrides.
 * Returns the { init, exit } exports.
 */
function loadPluginWithMocks(overrides = {}) {
    clearModuleCache();
    installMocks(overrides);
    const indexPath = path.resolve(__dirname, '..', 'index.js');
    return require(indexPath);
}

// ─── Tests ───────────────────────────────────────────────────────────────────

function registerTests() {
    // ── init() graceful degradation tests ────────────────────────────────────

    describe('init() never throws regardless of failure conditions', () => {

        it('resolves when binary is missing (getBinaryName returns null)', async () => {
            const plugin = loadPluginWithMocks({
                binary: {
                    getBinaryName: () => null,
                    checkForUpdate: async () => ({ updated: false, version: null }),
                },
            });
            const router = createMockRouter();

            // Should resolve without throwing
            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

        it('resolves when network is down (checkForUpdate throws)', async () => {
            const plugin = loadPluginWithMocks({
                binary: {
                    getBinaryName: () => 'eni-sidecar-darwin-arm64',
                    checkForUpdate: async () => { throw new Error('ENETUNREACH: network unreachable'); },
                },
            });
            const router = createMockRouter();

            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

        it('resolves when platform is unsupported (getBinaryName returns null)', async () => {
            const plugin = loadPluginWithMocks({
                binary: {
                    getBinaryName: () => null,
                    checkForUpdate: async () => { throw new Error('Unsupported platform'); },
                },
            });
            const router = createMockRouter();

            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

        it('resolves when UI extension install throws EACCES', async () => {
            const eaccesError = new Error('EACCES: permission denied');
            eaccesError.code = 'EACCES';

            const plugin = loadPluginWithMocks({
                uiInstaller: {
                    install: () => { throw eaccesError; },
                    needsUpdate: () => true,
                    getDataRoot: () => '/tmp/fake',
                    getTargetDir: () => '/tmp/fake/extensions/third-party/eni-world-builder',
                    copyDirSync: () => {},
                },
            });
            const router = createMockRouter();

            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

        it('resolves when sidecar spawn throws', async () => {
            const plugin = loadPluginWithMocks({
                binary: {
                    getBinaryName: () => 'eni-sidecar-darwin-arm64',
                    checkForUpdate: async () => ({ updated: false, version: '1.0.0' }),
                },
                sidecar: {
                    spawn: async () => { throw new Error('ENOENT: binary not found'); },
                    kill: async () => {},
                    enableCrashRecovery: () => {},
                    isRunning: () => false,
                    version: () => null,
                },
            });
            const router = createMockRouter();

            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

        it('resolves when ALL failures occur simultaneously', async () => {
            const eaccesError = new Error('EACCES: permission denied');
            eaccesError.code = 'EACCES';

            const plugin = loadPluginWithMocks({
                binary: {
                    getBinaryName: () => 'eni-sidecar-linux-x64',
                    checkForUpdate: async () => { throw new Error('ENETUNREACH: network unreachable'); },
                },
                sidecar: {
                    spawn: async () => { throw new Error('ENOENT: binary not found'); },
                    kill: async () => {},
                    enableCrashRecovery: () => {},
                    isRunning: () => false,
                    version: () => null,
                },
                uiInstaller: {
                    install: () => { throw eaccesError; },
                    needsUpdate: () => true,
                    getDataRoot: () => '/tmp/fake',
                    getTargetDir: () => '/tmp/fake/extensions/third-party/eni-world-builder',
                    copyDirSync: () => {},
                },
            });
            const router = createMockRouter();

            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

        it('resolves when routes registration throws', async () => {
            const plugin = loadPluginWithMocks({
                routes: function registerRoutes() {
                    throw new Error('Router registration failed');
                },
            });
            const router = createMockRouter();

            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

        it('resolves when sidecar spawn returns false (unhealthy)', async () => {
            const plugin = loadPluginWithMocks({
                binary: {
                    getBinaryName: () => 'eni-sidecar-darwin-arm64',
                    checkForUpdate: async () => ({ updated: false, version: '1.0.0' }),
                },
                sidecar: {
                    spawn: async () => false,
                    kill: async () => {},
                    enableCrashRecovery: () => {},
                    isRunning: () => false,
                    version: () => null,
                },
            });
            const router = createMockRouter();

            await assert.doesNotReject(async () => {
                await plugin.init(router);
            });
        });

    });

    // ── exit() graceful shutdown tests ───────────────────────────────────────

    describe('exit() always resolves without throwing', () => {

        it('resolves when no sidecar process is running', async () => {
            const plugin = loadPluginWithMocks({
                sidecar: {
                    kill: async () => {},
                    isRunning: () => false,
                    getProcess: () => null,
                },
            });

            await assert.doesNotReject(async () => {
                await plugin.exit();
            });
        });

        it('resolves when sidecar.kill() throws', async () => {
            const plugin = loadPluginWithMocks({
                sidecar: {
                    kill: async () => { throw new Error('Process already terminated'); },
                    isRunning: () => false,
                    getProcess: () => null,
                },
            });

            await assert.doesNotReject(async () => {
                await plugin.exit();
            });
        });

        it('resolves when sidecar process has already exited', async () => {
            const plugin = loadPluginWithMocks({
                sidecar: {
                    kill: async () => {
                        // Simulate killing a process that already exited — no-op
                    },
                    isRunning: () => false,
                    getProcess: () => ({ pid: 12345, exitCode: 0, signalCode: null }),
                },
            });

            await assert.doesNotReject(async () => {
                await plugin.exit();
            });
        });

        it('resolves when sidecar.kill() rejects with an error', async () => {
            const plugin = loadPluginWithMocks({
                sidecar: {
                    kill: () => Promise.reject(new Error('ESRCH: no such process')),
                    isRunning: () => false,
                    getProcess: () => null,
                },
            });

            await assert.doesNotReject(async () => {
                await plugin.exit();
            });
        });

    });

    // ── Summary ──────────────────────────────────────────────────────────────
}

async function runTests() {
    console.log('Property 5: Graceful Degradation — init/exit lifecycle tests');
    console.log('Validates: Requirements 6.5, 2.4');

    registerTests();
    await runQueue();

    console.log(`\n  ${passCount}/${testCount} tests passed`);

    if (passCount < testCount) {
        process.exitCode = 1;
    }
}

runTests().catch((err) => {
    console.error('Test runner error:', err);
    process.exitCode = 1;
});
