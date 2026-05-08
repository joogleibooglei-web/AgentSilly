/**
 * Unit tests for lib/routes.js — API Route Registration
 *
 * Tests the GET /status and POST /restart routes using a mock Express router.
 */

'use strict';

const { describe, it, beforeEach } = require('node:test');
const assert = require('node:assert/strict');
const path = require('path');

// Paths for cache manipulation
const sidecarPath = require.resolve('./sidecar');
const routesPath = require.resolve('./routes');

/**
 * Creates a mock Express response object that captures json() and status() calls.
 */
function createMockRes() {
    const res = {
        _json: null,
        _status: 200,
        json(data) {
            res._json = data;
            return res;
        },
        status(code) {
            res._status = code;
            return res;
        },
    };
    return res;
}

/**
 * Sets up the sidecar mock in require cache and returns a fresh routes module.
 */
function setupWithMock(sidecarOverrides = {}) {
    const defaultMock = {
        isRunning: () => true,
        version: () => '0.2.0',
        kill: async () => {},
        spawn: async () => true,
        enableCrashRecovery: () => {},
    };

    // Replace sidecar in require cache
    require.cache[sidecarPath] = {
        id: sidecarPath,
        filename: sidecarPath,
        loaded: true,
        exports: { ...defaultMock, ...sidecarOverrides },
    };

    // Clear routes cache so it picks up the mocked sidecar
    delete require.cache[routesPath];

    return require('./routes');
}

describe('lib/routes.js', () => {
    describe('registerRoutes', () => {
        it('should register GET /status and POST /restart routes', () => {
            const routes = setupWithMock();
            const registered = {};
            const router = {
                get: (p) => { registered[`GET ${p}`] = true; },
                post: (p) => { registered[`POST ${p}`] = true; },
            };

            routes.registerRoutes(router);

            assert.ok(registered['GET /status'], 'GET /status should be registered');
            assert.ok(registered['POST /restart'], 'POST /restart should be registered');
        });
    });

    describe('GET /status', () => {
        it('should return sidecar_running: true when sidecar is running', () => {
            const routes = setupWithMock({
                isRunning: () => true,
                version: () => '0.2.0',
            });

            const handler = captureHandler(routes, 'get', '/status');
            const res = createMockRes();
            handler({}, res);

            assert.deepEqual(res._json, {
                sidecar_running: true,
                sidecar_version: '0.2.0',
                plugin_version: '0.2.0',
            });
        });

        it('should return sidecar_running: false and sidecar_version: null when not running', () => {
            const routes = setupWithMock({
                isRunning: () => false,
                version: () => '0.2.0',
            });

            const handler = captureHandler(routes, 'get', '/status');
            const res = createMockRes();
            handler({}, res);

            assert.deepEqual(res._json, {
                sidecar_running: false,
                sidecar_version: null,
                plugin_version: '0.2.0',
            });
        });

        it('should include plugin_version from plugin.json', () => {
            const routes = setupWithMock();
            const handler = captureHandler(routes, 'get', '/status');
            const res = createMockRes();
            handler({}, res);

            assert.equal(res._json.plugin_version, '0.2.0');
        });
    });

    describe('POST /restart', () => {
        it('should kill and respawn sidecar, returning new status', async () => {
            let killCalled = false;
            let spawnCalled = false;
            let crashRecoveryCalled = false;

            const routes = setupWithMock({
                kill: async () => { killCalled = true; },
                spawn: async () => { spawnCalled = true; return true; },
                enableCrashRecovery: () => { crashRecoveryCalled = true; },
                isRunning: () => true,
                version: () => '0.2.0',
            });

            const handler = captureHandler(routes, 'post', '/restart');
            const res = createMockRes();
            await handler({}, res);

            assert.ok(killCalled, 'kill() should be called');
            assert.ok(spawnCalled, 'spawn() should be called');
            assert.ok(crashRecoveryCalled, 'enableCrashRecovery() should be called');
            assert.deepEqual(res._json, {
                sidecar_running: true,
                sidecar_version: '0.2.0',
                plugin_version: '0.2.0',
            });
        });

        it('should return 500 on error', async () => {
            const routes = setupWithMock({
                kill: async () => { throw new Error('kill failed'); },
            });

            const handler = captureHandler(routes, 'post', '/restart');
            const res = createMockRes();
            await handler({}, res);

            assert.equal(res._status, 500);
            assert.equal(res._json.error, 'Failed to restart sidecar');
            assert.equal(res._json.message, 'kill failed');
        });

        it('should return sidecar_running: false if spawn fails', async () => {
            const routes = setupWithMock({
                kill: async () => {},
                spawn: async () => false,
                enableCrashRecovery: () => {},
                isRunning: () => false,
                version: () => null,
            });

            const handler = captureHandler(routes, 'post', '/restart');
            const res = createMockRes();
            await handler({}, res);

            assert.deepEqual(res._json, {
                sidecar_running: false,
                sidecar_version: null,
                plugin_version: '0.2.0',
            });
        });
    });
});

/**
 * Registers routes on a mock router and returns the handler for the specified method/path.
 */
function captureHandler(routes, method, routePath) {
    let captured = null;
    const router = {
        get: (p, handler) => { if (p === routePath && method === 'get') captured = handler; },
        post: (p, handler) => { if (p === routePath && method === 'post') captured = handler; },
    };
    routes.registerRoutes(router);
    return captured;
}
