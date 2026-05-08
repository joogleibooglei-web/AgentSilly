/**
 * Property-Based Test: Version Comparison Transitivity
 *
 * Validates: Requirements 2.2
 *
 * Property 1: For all valid semver triples (a, b, c),
 * if compareVersions(a, b) < 0 and compareVersions(b, c) < 0,
 * then compareVersions(a, c) < 0.
 */

'use strict';

const assert = require('assert');
const { compareVersions } = require('./binary');

// --- Helpers ---

/**
 * Generate a random integer between 0 and max (inclusive).
 */
function randInt(max) {
    return Math.floor(Math.random() * (max + 1));
}

/**
 * Generate a random valid semver string with components 0-99.
 */
function randomVersion() {
    return `${randInt(99)}.${randInt(99)}.${randInt(99)}`;
}

// --- Property Tests ---

const ITERATIONS = 200;

/**
 * Property 1: Transitivity
 * If compareVersions(a, b) < 0 AND compareVersions(b, c) < 0,
 * then compareVersions(a, c) < 0.
 */
function testTransitivity() {
    let tested = 0;

    for (let i = 0; i < ITERATIONS * 10; i++) {
        const a = randomVersion();
        const b = randomVersion();
        const c = randomVersion();

        const ab = compareVersions(a, b);
        const bc = compareVersions(b, c);

        if (ab < 0 && bc < 0) {
            const ac = compareVersions(a, c);
            assert.strictEqual(
                ac < 0,
                true,
                `Transitivity violated: compareVersions("${a}", "${b}") = ${ab}, ` +
                `compareVersions("${b}", "${c}") = ${bc}, ` +
                `but compareVersions("${a}", "${c}") = ${ac} (expected < 0)`
            );
            tested++;
        }

        if (tested >= ITERATIONS) break;
    }

    assert(tested >= 50, `Not enough transitive triples found (only ${tested}). Increase iterations.`);
    console.log(`  ✓ Transitivity: ${tested} triples verified`);
}

/**
 * Property: Reflexivity
 * compareVersions(a, a) === 0 for any valid version.
 */
function testReflexivity() {
    for (let i = 0; i < ITERATIONS; i++) {
        const a = randomVersion();
        const result = compareVersions(a, a);
        assert.strictEqual(
            result,
            0,
            `Reflexivity violated: compareVersions("${a}", "${a}") = ${result} (expected 0)`
        );
    }
    console.log(`  ✓ Reflexivity: ${ITERATIONS} versions verified`);
}

/**
 * Property: Anti-symmetry
 * If compareVersions(a, b) < 0, then compareVersions(b, a) > 0.
 */
function testAntiSymmetry() {
    let tested = 0;

    for (let i = 0; i < ITERATIONS * 5; i++) {
        const a = randomVersion();
        const b = randomVersion();

        const ab = compareVersions(a, b);

        if (ab < 0) {
            const ba = compareVersions(b, a);
            assert.strictEqual(
                ba > 0,
                true,
                `Anti-symmetry violated: compareVersions("${a}", "${b}") = ${ab}, ` +
                `but compareVersions("${b}", "${a}") = ${ba} (expected > 0)`
            );
            tested++;
        } else if (ab > 0) {
            const ba = compareVersions(b, a);
            assert.strictEqual(
                ba < 0,
                true,
                `Anti-symmetry violated: compareVersions("${a}", "${b}") = ${ab}, ` +
                `but compareVersions("${b}", "${a}") = ${ba} (expected < 0)`
            );
            tested++;
        }

        if (tested >= ITERATIONS) break;
    }

    assert(tested >= 50, `Not enough anti-symmetric pairs found (only ${tested}).`);
    console.log(`  ✓ Anti-symmetry: ${tested} pairs verified`);
}

// --- Known Examples ---

function testKnownExamples() {
    // "1.0.0" < "2.0.0"
    assert.strictEqual(
        compareVersions('1.0.0', '2.0.0'),
        -1,
        'Expected "1.0.0" < "2.0.0"'
    );

    // "1.2.3" < "1.2.4"
    assert.strictEqual(
        compareVersions('1.2.3', '1.2.4'),
        -1,
        'Expected "1.2.3" < "1.2.4"'
    );

    // "0.9.9" < "1.0.0"
    assert.strictEqual(
        compareVersions('0.9.9', '1.0.0'),
        -1,
        'Expected "0.9.9" < "1.0.0"'
    );

    // Equal versions
    assert.strictEqual(
        compareVersions('1.2.3', '1.2.3'),
        0,
        'Expected "1.2.3" === "1.2.3"'
    );

    // Greater version
    assert.strictEqual(
        compareVersions('2.0.0', '1.9.9'),
        1,
        'Expected "2.0.0" > "1.9.9"'
    );

    console.log('  ✓ Known examples: all passed');
}

// --- Edge Cases ---

function testEdgeCases() {
    // Invalid version strings should return 0
    assert.strictEqual(
        compareVersions('invalid', '1.0.0'),
        0,
        'Expected invalid version to return 0'
    );

    assert.strictEqual(
        compareVersions('1.0.0', 'not-a-version'),
        0,
        'Expected invalid version to return 0'
    );

    assert.strictEqual(
        compareVersions('', '1.0.0'),
        0,
        'Expected empty string to return 0'
    );

    assert.strictEqual(
        compareVersions('1.0.0', ''),
        0,
        'Expected empty string to return 0'
    );

    assert.strictEqual(
        compareVersions('abc', 'xyz'),
        0,
        'Expected both invalid to return 0'
    );

    assert.strictEqual(
        compareVersions('1.0', '1.0.0'),
        0,
        'Expected incomplete version to return 0'
    );

    assert.strictEqual(
        compareVersions('1.0.0.0', '1.0.0'),
        0,
        'Expected extra component version to return 0'
    );

    assert.strictEqual(
        compareVersions(null, '1.0.0'),
        0,
        'Expected null to return 0'
    );

    assert.strictEqual(
        compareVersions(undefined, '1.0.0'),
        0,
        'Expected undefined to return 0'
    );

    console.log('  ✓ Edge cases: all passed');
}

// --- Run All Tests ---

console.log('Property-Based Test: Version Comparison (Property 1)');
console.log('Validates: Requirements 2.2\n');

testTransitivity();
testReflexivity();
testAntiSymmetry();
testKnownExamples();
testEdgeCases();

console.log('\n✓ All version comparison property tests passed');
