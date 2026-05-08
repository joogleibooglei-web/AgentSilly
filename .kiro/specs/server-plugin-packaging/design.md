# Design Document: Server Plugin Packaging

## Overview

This design restructures the ENI World Builder from a UI-extension-only package into a proper SillyTavern Server Plugin that serves as the primary install target. The plugin manages the Rust sidecar binary lifecycle and auto-installs the UI extension, providing a single-step installation experience.

## Architecture

```
plugins/eni-world-builder/          ← Primary install target (git clone here)
├── index.js                        ← Server Plugin entry point
├── plugin.json                     ← ST server plugin manifest
├── config.json                     ← Optional user configuration
├── manifest.json                   ← UI extension manifest (source of truth)
├── dist/                           ← Built frontend assets
│   ├── index.js
│   └── index.css
├── bin/                            ← Downloaded sidecar binaries (gitignored)
│   └── eni-sidecar-{platform-arch}
├── frontend/                       ← Svelte source (dev only)
├── eni-sidecar/                    ← Rust source (dev only)
└── .github/workflows/              ← CI for building binaries

data/default/extensions/third-party/eni-world-builder/  ← Auto-installed by plugin
├── manifest.json                   ← Copied from plugin dir
└── dist/
    ├── index.js                    ← Copied from plugin dir
    └── index.css
```

## Module Decomposition

The `index.js` entry point will be refactored into focused modules:

### `index.js` — Plugin Entry Point
- Exports `init(router)` and `exit()` per ST server plugin contract
- Orchestrates initialization sequence: config → download → spawn → install UI → register routes
- Handles top-level error catching to prevent crashing ST

### `lib/config.js` — Configuration Manager
- Loads `config.json` from plugin directory
- Merges with defaults
- Exports a frozen config object

### `lib/binary.js` — Binary Download & Version Management
- `getBinaryName()` — platform-specific binary filename
- `getBinaryPath()` — full path to binary in `bin/`
- `getLocalVersion()` — runs binary with `--version`
- `fetchLatestRelease()` — GitHub API with rate-limit retry
- `downloadBinary()` — download with corruption detection and retry
- `checkForUpdate()` — compare local vs remote version

### `lib/sidecar.js` — Process Lifecycle Manager
- `spawn()` — start the sidecar child process
- `kill()` — graceful shutdown (SIGTERM → SIGKILL / taskkill on Windows)
- `waitForHealth()` — poll health endpoint
- `isAlreadyRunning()` — check if port is in use
- Crash recovery with rate-limited restarts (5 per 60s window)
- Exposes state: `isRunning`, `version`, `pid`

### `lib/ui-installer.js` — UI Extension Installer
- `install()` — copy manifest.json + dist/ to extensions directory
- `needsUpdate()` — compare installed version vs plugin version
- Resolves ST data root from ST's runtime context or defaults

### `lib/routes.js` — API Route Registration
- `GET /status` — returns sidecar state + versions
- `POST /restart` — triggers sidecar restart cycle

## Key Design Decisions

### 1. ST Data Root Discovery

SillyTavern passes the Express router to `init(router)` but does not directly expose the data directory path. The plugin will:
1. Check for `global.DATA_ROOT` (some ST versions expose this)
2. Fall back to resolving `../../data/default/` relative to the plugins directory (standard ST layout)
3. Allow override via `config.json` → `dataRoot` field

### 2. Version Comparison Strategy

The plugin uses semver comparison (major.minor.patch) to determine if an update is needed. The local binary version is obtained by running `binary --version` and parsing the output. This avoids maintaining a separate version file.

### 3. Crash Recovery Rate Limiting

A sliding window approach tracks restart timestamps. When a restart is attempted, timestamps older than 60 seconds are pruned. If 5 or more timestamps remain, the restart is suppressed and an error is logged. This prevents infinite restart loops from consuming resources.

### 4. Windows Process Termination

On Windows, `process.kill(pid, 'SIGTERM')` doesn't work reliably. The plugin will use:
- `sidecarProcess.kill()` (sends TerminateProcess on Windows in Node.js)
- As fallback: `spawn('taskkill', ['/pid', pid, '/f'])` for force-kill scenarios

### 5. UI Extension Installation Timing

The UI extension is installed during `init()` before routes are registered. This ensures the frontend is available by the time a user loads the ST web interface. The copy is skipped if versions match to avoid unnecessary I/O on every restart.

## Initialization Sequence

```
init(router) called by SillyTavern
  │
  ├─ 1. Load config.json (or defaults)
  │
  ├─ 2. Check platform support
  │     └─ If unsupported → log error, continue in degraded mode
  │
  ├─ 3. Binary management
  │     ├─ If binary missing → download from GitHub Release
  │     ├─ If binary exists & autoUpdate enabled → check for update
  │     └─ On any failure → log error, continue if binary exists
  │
  ├─ 4. Sidecar lifecycle
  │     ├─ If already running on port → skip spawn, attach monitoring
  │     ├─ If binary available → spawn, wait for health
  │     └─ Set up crash recovery monitoring
  │
  ├─ 5. UI extension installation
  │     ├─ Resolve ST data root
  │     ├─ Check installed version
  │     └─ Copy files if needed
  │
  └─ 6. Register API routes on router
        ├─ GET /status
        └─ POST /restart
```

## Shutdown Sequence

```
exit() called by SillyTavern
  │
  ├─ 1. Disable crash recovery (prevent restart during shutdown)
  │
  ├─ 2. Send SIGTERM to sidecar (or taskkill /pid on Windows)
  │
  ├─ 3. Wait up to 5 seconds for process exit
  │
  └─ 4. If still running → SIGKILL (or taskkill /f on Windows)
```

## File: `plugin.json`

```json
{
  "id": "eni-world-builder",
  "name": "ENI World Builder",
  "description": "AI-powered world building assistant with Rust sidecar backend",
  "version": "0.2.0"
}
```

## File: `config.json` (optional, user-created)

```json
{
  "sidecarPort": 7842,
  "autoUpdate": true,
  "dataRoot": null
}
```

## Correctness Properties

### Property 1: Version Comparison Transitivity
- **Requirement:** 2.2
- **Criteria:** Version comparison is consistent — if A < B and B < C then A < C
- **Property:** For all valid semver triples (a, b, c), if compareVersions(a, b) < 0 and compareVersions(b, c) < 0, then compareVersions(a, c) < 0
- **Type:** Property-based test (metamorphic)

### Property 2: Platform Identifier Mapping Completeness
- **Requirement:** 2.3, 7.4
- **Criteria:** Every supported platform/arch combination maps to a valid binary name
- **Property:** For all supported (platform, arch) pairs, getBinaryName() returns a non-empty string matching the pattern `eni-sidecar-{platform}-{arch}[.exe]`
- **Type:** Example-based test (finite set of 4 platforms)

### Property 3: Restart Rate Limiting Invariant
- **Requirement:** 3.4
- **Criteria:** No more than 5 restarts occur within any 60-second window
- **Property:** For any sequence of crash events with arbitrary timestamps, the number of actual restarts in any 60-second sliding window never exceeds 5
- **Type:** Property-based test (invariant)

### Property 4: Config Merge Idempotence
- **Requirement:** 6.1, 6.2, 6.3
- **Criteria:** Loading config produces a complete config object regardless of input
- **Property:** For any partial config object (including empty), merging with defaults produces an object with all required keys and valid types
- **Type:** Property-based test (idempotence of merge with defaults)

### Property 5: Graceful Degradation
- **Requirement:** 6.5, 2.4
- **Criteria:** The plugin never throws from init() regardless of failure conditions
- **Property:** For any combination of failures (network down, binary missing, permissions error), init() resolves without throwing
- **Type:** Example-based test (error injection scenarios)

### Property 6: UI Install Version Skip
- **Requirement:** 4.2, 4.3
- **Criteria:** Copy is skipped when versions match, performed when they differ
- **Property:** For any two version strings, needsUpdate(installed, current) returns true if and only if installed !== current
- **Type:** Example-based test

## Testing Strategy

- **Unit tests** for pure functions: version comparison, platform identifier, config merge
- **Integration tests** with mocked fs/network for: download flow, UI installation, process lifecycle
- **Property-based tests** for: version comparison transitivity, restart rate limiting invariant, config merge completeness
- **Manual testing** for: end-to-end ST plugin loading, actual sidecar spawn, cross-platform verification
