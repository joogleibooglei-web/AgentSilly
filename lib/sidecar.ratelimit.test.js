/**
 * Property-based test for restart rate limiting (Property 3)
 *
 * **Validates: Requirements 3.4**
 *
 * Property 3: Restart Rate Limiting Invariant
 * For any sequence of crash events with arbitrary timestamps, the number of
 * actual restarts in any 60-second sliding window never exceeds 5.
 *
 * Uses Node.js built-in assert module. Run with: node lib/sidecar.ratelimit.test.js
 */

'use strict';

const assert = require('assert');

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
    }
}

/**
 * Re-require the sidecar module to get a fresh instance.
 */
function freshRequire() {
    delete require.cache[require.resolve('./sidecar.js')];
    return require('./sidecar.js');
}

/**
 * Generate a random integer between min (inclusive) and max (inclusive).
 */
function randomInt(min, max) {
    return Math.floor(Math.random() * (max - min + 1)) + min;
}

/**
 * Generate a random sequence of crash timestamps.
 * Timestamps span a range of 0 to 300 seconds (5 minutes) from a base time.
 *
 * @param {number} count - Number of timestamps to generate
 * @param {number} baseTime - Base time in milliseconds
 * @returns {number[]} Sorted array of timestamps
 */
function generateCrashTimestamps(count, baseTime) {
    const timestamps = [];
    for (let i = 0; i < count; i++) {
        // Random offset 0-300 seconds from base
        const offsetMs = randomInt(0, 300000);
        timestamps.push(baseTime + offsetMs);
    }
    timestamps.sort((a, b) => a - b);
    return timestamps;
}

/**
 * Simulate the rate limiter for a sequence of crash timestamps.
 * Mocks Date.now() to control time, then calls isRestartAllowed() at each timestamp.
 * Records which crashes resulted in actual restarts.
 *
 * @param {number[]} crashTimestamps - Sorted array of crash event timestamps
 * @returns {number[]} Array of timestamps where restarts were actually allowed
 */
function simulateRateLimiter(crashTimestamps) {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    const actualRestarts = [];
    const originalDateNow = Date.now;

    try {
        for (const ts of crashTimestamps) {
            // Mock Date.now() to return the current crash timestamp
            Date.now = () => ts;

            if (sidecar.isRestartAllowed()) {
                // Restart is allowed — record it and add to timestamps
                // (simulating what enableCrashRecovery does: push Date.now())
                actualRestarts.push(ts);
                const currentTimestamps = sidecar.getRestartTimestamps();
                currentTimestamps.push(ts);
                sidecar.setRestartTimestamps(currentTimestamps);
            }
        }
    } finally {
        Date.now = originalDateNow;
    }

    return actualRestarts;
}

/**
 * Check the invariant: in any 60-second window, no more than 5 restarts occurred.
 *
 * @param {number[]} restartTimestamps - Sorted array of actual restart timestamps
 * @returns {{ valid: boolean, windowStart?: number, windowEnd?: number, count?: number }}
 */
function checkSlidingWindowInvariant(restartTimestamps) {
    const windowMs = 60000;

    for (let i = 0; i < restartTimestamps.length; i++) {
        const windowStart = restartTimestamps[i];
        const windowEnd = windowStart + windowMs;

        // Count restarts within [windowStart, windowEnd)
        let count = 0;
        for (let j = i; j < restartTimestamps.length; j++) {
            if (restartTimestamps[j] < windowEnd) {
                count++;
            } else {
                break;
            }
        }

        if (count > 5) {
            return { valid: false, windowStart, windowEnd, count };
        }
    }

    return { valid: true };
}

// ============================================================
// Property-based test: random sequences
// ============================================================

console.log('Property 3: Restart Rate Limiting Invariant');
console.log('  Property-based tests (random sequences)\n');

const NUM_ITERATIONS = 100;
const baseTime = 1700000000000; // Fixed base time for reproducibility

test(`invariant holds for ${NUM_ITERATIONS} random crash sequences`, () => {
    for (let i = 0; i < NUM_ITERATIONS; i++) {
        const numCrashes = randomInt(1, 20);
        const crashTimestamps = generateCrashTimestamps(numCrashes, baseTime);
        const actualRestarts = simulateRateLimiter(crashTimestamps);
        const result = checkSlidingWindowInvariant(actualRestarts);

        assert.ok(
            result.valid,
            `Iteration ${i}: invariant violated! ` +
            `${result.count} restarts in 60s window ` +
            `[${result.windowStart}, ${result.windowEnd}). ` +
            `Crash sequence (${numCrashes} events): [${crashTimestamps.join(', ')}]. ` +
            `Actual restarts: [${actualRestarts.join(', ')}]`
        );
    }
});

// ============================================================
// Specific scenario tests
// ============================================================

console.log('\n  Specific scenarios\n');

test('5 restarts within 60s → 6th is suppressed', () => {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    const now = 1700000060000;
    const originalDateNow = Date.now;

    try {
        // Simulate 5 restarts at 10s intervals within 60s
        const timestamps = [
            now - 50000,
            now - 40000,
            now - 30000,
            now - 20000,
            now - 10000,
        ];

        // Set timestamps as if 5 restarts already happened
        sidecar.setRestartTimestamps(timestamps);

        // Mock Date.now to current time
        Date.now = () => now;

        // 6th restart should be suppressed
        const allowed = sidecar.isRestartAllowed();
        assert.strictEqual(allowed, false, 'Expected 6th restart to be suppressed');
    } finally {
        Date.now = originalDateNow;
    }
});

test('after 60s passes, restarts are allowed again', () => {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    const originalDateNow = Date.now;

    try {
        const baseTs = 1700000000000;

        // Set 5 timestamps all at baseTs
        sidecar.setRestartTimestamps([
            baseTs,
            baseTs + 1000,
            baseTs + 2000,
            baseTs + 3000,
            baseTs + 4000,
        ]);

        // At baseTs + 30s, should still be suppressed
        Date.now = () => baseTs + 30000;
        assert.strictEqual(sidecar.isRestartAllowed(), false, 'Should be suppressed at 30s');

        // At baseTs + 61s, all timestamps are older than 60s, should be allowed
        Date.now = () => baseTs + 61000;
        assert.strictEqual(sidecar.isRestartAllowed(), true, 'Should be allowed after 60s');
    } finally {
        Date.now = originalDateNow;
    }
});

test('empty timestamps → restart allowed', () => {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    // With no timestamps, restart should always be allowed
    const allowed = sidecar.isRestartAllowed();
    assert.strictEqual(allowed, true, 'Expected restart to be allowed with empty timestamps');
});

test('exactly 5 timestamps within window → next is suppressed', () => {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    const originalDateNow = Date.now;

    try {
        const now = 1700000060000;
        Date.now = () => now;

        // Set exactly 5 timestamps within the 60s window
        sidecar.setRestartTimestamps([
            now - 55000,
            now - 45000,
            now - 35000,
            now - 25000,
            now - 15000,
        ]);

        // With exactly 5 in the window, next should be suppressed
        const allowed = sidecar.isRestartAllowed();
        assert.strictEqual(allowed, false, 'Expected restart to be suppressed with exactly 5 in window');
    } finally {
        Date.now = originalDateNow;
    }
});

test('4 timestamps within window → restart still allowed', () => {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    const originalDateNow = Date.now;

    try {
        const now = 1700000060000;
        Date.now = () => now;

        // Set 4 timestamps within the 60s window
        sidecar.setRestartTimestamps([
            now - 55000,
            now - 45000,
            now - 35000,
            now - 25000,
        ]);

        // With 4 in the window, next should be allowed
        const allowed = sidecar.isRestartAllowed();
        assert.strictEqual(allowed, true, 'Expected restart to be allowed with only 4 in window');
    } finally {
        Date.now = originalDateNow;
    }
});

// ============================================================
// Additional property: stress test with rapid-fire crashes
// ============================================================

console.log('\n  Stress tests\n');

test('rapid-fire: 20 crashes in 1 second still limited to 5 restarts', () => {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    const originalDateNow = Date.now;
    const actualRestarts = [];

    try {
        const baseTs = 1700000000000;

        // 20 crashes within 1 second
        for (let i = 0; i < 20; i++) {
            const ts = baseTs + i * 50; // 50ms apart
            Date.now = () => ts;

            if (sidecar.isRestartAllowed()) {
                actualRestarts.push(ts);
                const current = sidecar.getRestartTimestamps();
                current.push(ts);
                sidecar.setRestartTimestamps(current);
            }
        }
    } finally {
        Date.now = originalDateNow;
    }

    assert.strictEqual(actualRestarts.length, 5, `Expected exactly 5 restarts, got ${actualRestarts.length}`);
    const result = checkSlidingWindowInvariant(actualRestarts);
    assert.ok(result.valid, 'Sliding window invariant violated in stress test');
});

test('spread over 5 minutes: restarts allowed in each 60s window', () => {
    const sidecar = freshRequire();
    sidecar.resetRestartTimestamps();

    const originalDateNow = Date.now;
    const actualRestarts = [];

    try {
        const baseTs = 1700000000000;

        // 15 crashes spread over 5 minutes (one every 20 seconds)
        for (let i = 0; i < 15; i++) {
            const ts = baseTs + i * 20000; // 20s apart
            Date.now = () => ts;

            if (sidecar.isRestartAllowed()) {
                actualRestarts.push(ts);
                const current = sidecar.getRestartTimestamps();
                current.push(ts);
                sidecar.setRestartTimestamps(current);
            }
        }
    } finally {
        Date.now = originalDateNow;
    }

    // Verify invariant
    const result = checkSlidingWindowInvariant(actualRestarts);
    assert.ok(result.valid, `Sliding window invariant violated: ${result.count} restarts in window`);

    // Should have more than 5 total restarts since they span multiple windows
    assert.ok(actualRestarts.length > 5, `Expected more than 5 total restarts over 5 minutes, got ${actualRestarts.length}`);
});

// ============================================================
// Summary
// ============================================================

console.log(`\n${passed + failed} tests: ${passed} passed, ${failed} failed`);
if (failed > 0) {
    process.exit(1);
}
