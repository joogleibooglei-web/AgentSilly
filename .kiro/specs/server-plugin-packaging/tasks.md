# Implementation Plan: Server Plugin Packaging

## Overview

Restructure the ENI World Builder from a UI-extension-only package into a SillyTavern Server Plugin. The plugin manages the Rust sidecar binary lifecycle and auto-installs the UI extension, providing a single-step installation experience. Implementation is in JavaScript (Node.js) following the module decomposition in the design document.

## Tasks

- [x] 1. Set up plugin structure and configuration module
  - [x] 1.1 Create plugin directory structure and manifests
    - Create `plugin.json` with id, name, description, version fields
    - Create default `config.json` with sidecarPort (7842), autoUpdate (true), dataRoot (null)
    - Add `bin/` to `.gitignore`
    - _Requirements: 1.1, 1.2, 1.3_

  - [x] 1.2 Implement `lib/config.js` configuration manager
    - Load `config.json` from plugin directory using `fs.readFileSync`
    - Merge user config with defaults (sidecarPort: 7842, autoUpdate: true, dataRoot: null)
    - Export a frozen config object
    - Handle missing or malformed config.json gracefully (fall back to defaults)
    - _Requirements: 6.1, 6.2, 6.3, 6.5_

  - [x] 1.3 Write property test for config merge (Property 4)
    - **Property 4: Config Merge Idempotence**
    - For any partial config object (including empty), merging with defaults produces an object with all required keys and valid types
    - **Validates: Requirements 6.1, 6.2, 6.3**

- [x] 2. Implement binary download and version management
  - [x] 2.1 Implement `lib/binary.js` — platform detection and path utilities
    - `getBinaryName()` — return platform-specific binary filename using `process.platform` and `process.arch`
    - Append `.exe` on Windows
    - `getBinaryPath()` — return full path to binary in `bin/` directory
    - Support platforms: darwin-arm64, darwin-x64, linux-x64, win32-x64
    - Log error for unsupported platforms
    - _Requirements: 2.3, 7.3, 7.4_

  - [x] 2.2 Write property test for platform identifier mapping (Property 2)
    - **Property 2: Platform Identifier Mapping Completeness**
    - For all supported (platform, arch) pairs, getBinaryName() returns a non-empty string matching `eni-sidecar-{platform}-{arch}[.exe]`
    - **Validates: Requirements 2.3, 7.4**

  - [x] 2.3 Implement `lib/binary.js` — version comparison utilities
    - `getLocalVersion()` — run binary with `--version` flag and parse semver output
    - `compareVersions(a, b)` — semver comparison returning -1, 0, or 1
    - Handle invalid version strings gracefully
    - _Requirements: 2.2_

  - [x] 2.4 Write property test for version comparison (Property 1)
    - **Property 1: Version Comparison Transitivity**
    - For all valid semver triples (a, b, c), if compareVersions(a, b) < 0 and compareVersions(b, c) < 0, then compareVersions(a, c) < 0
    - **Validates: Requirements 2.2**

  - [x] 2.5 Implement `lib/binary.js` — GitHub release fetching and download
    - `fetchLatestRelease()` — call GitHub API, handle 403/429 with exponential backoff (up to 3 retries)
    - `downloadBinary()` — download asset, verify file size (discard if < 50% expected), retry once on corruption
    - Set executable permission (0o755) on macOS/Linux after download
    - `checkForUpdate()` — compare local vs remote version, trigger download if remote is newer
    - Log manual download URL on network failure
    - _Requirements: 2.1, 2.2, 2.4, 2.5, 2.6, 2.7, 7.2_

- [x] 3. Checkpoint - Ensure config and binary modules work
  - Ensure all tests pass, ask the user if questions arise.

- [x] 4. Implement sidecar process lifecycle
  - [x] 4.1 Implement `lib/sidecar.js` — spawn and health check
    - `spawn()` — start sidecar as child process with configured port
    - `waitForHealth()` — poll `http://127.0.0.1:{port}/health` up to 10 seconds for 200 response
    - `isAlreadyRunning()` — check if port is in use, skip spawn if so
    - Prefix all log messages with `[ENI]`
    - _Requirements: 3.1, 3.2, 3.6, 6.4_

  - [x] 4.2 Implement `lib/sidecar.js` — shutdown and crash recovery
    - `kill()` — send SIGTERM, wait 5s, then SIGKILL; use taskkill on Windows
    - Monitor child process exit events for unexpected crashes
    - Implement sliding window rate limiter: track restart timestamps, prune > 60s old, suppress if >= 5 remain
    - Wait 2 seconds before restart attempt
    - Expose state: `isRunning`, `version`, `pid`
    - _Requirements: 3.3, 3.4, 3.5, 7.1_

  - [x] 4.3 Write property test for restart rate limiting (Property 3)
    - **Property 3: Restart Rate Limiting Invariant**
    - For any sequence of crash events with arbitrary timestamps, the number of actual restarts in any 60-second sliding window never exceeds 5
    - **Validates: Requirements 3.4**

- [x] 5. Implement UI extension auto-installation
  - [x] 5.1 Implement `lib/ui-installer.js`
    - `install()` — copy `manifest.json` and `dist/` to `<ST_Data_Root>/extensions/third-party/eni-world-builder/`
    - `needsUpdate()` — compare installed manifest version vs plugin manifest version
    - Resolve ST data root: check `global.DATA_ROOT`, fall back to `../../data/default/` relative to plugins dir, allow config override
    - Create target directory recursively if it doesn't exist
    - Skip copy if versions match
    - Log descriptive error on permissions failure and continue
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

  - [x] 5.2 Write unit tests for UI installer
    - Test needsUpdate() returns true when versions differ, false when they match (Property 6)
    - Test directory creation when target doesn't exist
    - Test graceful handling of permissions errors
    - **Validates: Requirements 4.2, 4.3, 4.4, 4.5**

- [x] 6. Checkpoint - Ensure sidecar and UI installer modules work
  - Ensure all tests pass, ask the user if questions arise.

- [x] 7. Implement API routes and plugin entry point
  - [x] 7.1 Implement `lib/routes.js` — API route registration
    - `GET /status` — return JSON with `sidecar_running` (boolean), `sidecar_version` (string|null), `plugin_version` (string)
    - `POST /restart` — kill current sidecar, spawn new one, return new health status
    - _Requirements: 5.1, 5.2, 5.3, 5.4_

  - [x] 7.2 Implement `index.js` — plugin entry point
    - Export `init(router)` — orchestrate: config → platform check → download → spawn → install UI → register routes
    - Export `exit()` — disable crash recovery, kill sidecar gracefully
    - Wrap all init logic in try/catch to never crash SillyTavern
    - Log all errors at error level with `[ENI]` prefix
    - _Requirements: 1.1, 1.3, 6.4, 6.5_

  - [x] 7.3 Write integration tests for init/exit lifecycle (Property 5)
    - **Property 5: Graceful Degradation**
    - For any combination of failures (network down, binary missing, permissions error), init() resolves without throwing
    - Test exit() properly shuts down sidecar
    - **Validates: Requirements 6.5, 2.4**

- [x] 8. Final checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- The implementation language is JavaScript (Node.js) as specified in the design
- All file paths use `path.join()` for cross-platform compatibility (Requirement 7.3)

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2"] },
    { "id": 1, "tasks": ["1.3", "2.1"] },
    { "id": 2, "tasks": ["2.2", "2.3"] },
    { "id": 3, "tasks": ["2.4", "2.5"] },
    { "id": 4, "tasks": ["4.1", "5.1"] },
    { "id": 5, "tasks": ["4.2", "4.3", "5.2"] },
    { "id": 6, "tasks": ["7.1", "7.2"] },
    { "id": 7, "tasks": ["7.3"] }
  ]
}
```
