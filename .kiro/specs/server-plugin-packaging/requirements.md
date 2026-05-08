# Requirements Document

## Introduction

This document specifies the requirements for restructuring the ENI World Builder extension packaging for SillyTavern v0.2.0. The current architecture places all code in a UI extension directory, but the server-side plugin (index.js) is never loaded by SillyTavern because it resides in the wrong location. The v0.2.0 release restructures the project so that a **Server Plugin** is the primary install target. The Server Plugin manages the Rust sidecar binary lifecycle and auto-installs the UI extension assets into the correct directory, providing a single-step installation experience for end users.

## Glossary

- **Server_Plugin**: A Node.js plugin installed in SillyTavern's `plugins/` directory, loaded when `enableServerPlugins: true` is set in `config.yaml`. Exports `init(router)` and `exit()` lifecycle hooks.
- **UI_Extension**: A browser-side extension installed in `data/<user>/extensions/third-party/`, consisting of a `manifest.json` and bundled JS/CSS assets.
- **Sidecar**: The Rust binary (`eni-sidecar`) that provides the AI agent backend, communicating with the UI via WebSocket on port 7842.
- **Plugin_Router**: The Express router instance passed to a Server Plugin's `init()` function for registering HTTP API routes under the plugin's namespace.
- **Health_Endpoint**: The HTTP endpoint (`http://127.0.0.1:7842/health`) exposed by the Sidecar for liveness checks.
- **GitHub_Release**: A tagged release on the GitHub repository containing platform-specific Sidecar binaries as downloadable assets.
- **Platform_Identifier**: A string combining `process.platform` and `process.arch` (e.g., `darwin-arm64`, `linux-x64`, `win32-x64`) used to select the correct binary.
- **ST_Data_Root**: The SillyTavern data root directory, typically `data/default/` for single-user setups, containing the `extensions/third-party/` subdirectory.

## Requirements

### Requirement 1: Plugin Directory Structure

**User Story:** As a SillyTavern user, I want to install the ENI World Builder by cloning a single repository into the `plugins/` directory, so that both the backend and frontend are set up automatically.

#### Acceptance Criteria

1. THE Server_Plugin SHALL export an `init(router)` function and an `exit()` function as its module interface.
2. THE Server_Plugin SHALL include a `plugin.json` manifest with fields: `id`, `name`, `description`, and `version`.
3. WHEN SillyTavern loads the Server_Plugin, THE Server_Plugin SHALL be loadable from the `plugins/eni-world-builder/` directory without requiring any files in the UI extension directory.

### Requirement 2: Sidecar Binary Download

**User Story:** As a user, I want the plugin to automatically download the correct sidecar binary for my platform, so that I don't need to manually fetch it from GitHub.

#### Acceptance Criteria

1. WHEN the Server_Plugin initializes and no Sidecar binary exists locally, THE Server_Plugin SHALL download the platform-appropriate binary from the latest GitHub_Release.
2. WHEN the Server_Plugin initializes and a Sidecar binary exists locally, THE Server_Plugin SHALL compare the local binary version against the latest GitHub_Release version and download an update if the remote version is newer.
3. THE Server_Plugin SHALL select the correct binary asset using the Platform_Identifier derived from `process.platform` and `process.arch`.
4. IF the download fails due to a network error, THEN THE Server_Plugin SHALL log a descriptive error message including a manual download URL and continue initialization without the Sidecar.
5. IF the GitHub API returns a 403 or 429 status code, THEN THE Server_Plugin SHALL retry with exponential backoff up to 3 attempts before reporting a rate-limit error.
6. IF the downloaded file size is less than 50% of the expected asset size, THEN THE Server_Plugin SHALL discard the file, retry the download once, and report a corruption error if the retry also fails.
7. IF the current Platform_Identifier does not match any supported platform, THEN THE Server_Plugin SHALL log an error identifying the unsupported platform and skip the download.

### Requirement 3: Sidecar Process Lifecycle

**User Story:** As a user, I want the sidecar to start automatically and recover from crashes, so that the world builder backend is always available while SillyTavern is running.

#### Acceptance Criteria

1. WHEN the Sidecar binary is present and no existing Sidecar process is detected on port 7842, THE Server_Plugin SHALL spawn the Sidecar as a child process.
2. WHEN the Sidecar process is spawned, THE Server_Plugin SHALL wait up to 10 seconds for the Health_Endpoint to return a 200 response before reporting a startup failure.
3. WHILE the Server_Plugin is running, THE Server_Plugin SHALL monitor the Sidecar process and restart it if the process exits unexpectedly.
4. WHEN the Sidecar process exits unexpectedly, THE Server_Plugin SHALL wait 2 seconds before attempting a restart and limit restart attempts to 5 within any 60-second window.
5. WHEN SillyTavern shuts down, THE Server_Plugin SHALL send SIGTERM to the Sidecar process and wait up to 5 seconds for graceful termination before sending SIGKILL.
6. WHEN the Server_Plugin detects that a Sidecar is already running on port 7842 during initialization, THE Server_Plugin SHALL skip spawning a new process and log that an existing instance was found.

### Requirement 4: UI Extension Auto-Installation

**User Story:** As a user, I want the server plugin to automatically install the UI extension files, so that I don't need to separately clone or copy the frontend into the extensions directory.

#### Acceptance Criteria

1. WHEN the Server_Plugin initializes, THE Server_Plugin SHALL copy `manifest.json` and the `dist/` directory into `<ST_Data_Root>/extensions/third-party/eni-world-builder/`.
2. WHEN the UI extension files already exist at the target path and the installed version matches the plugin version, THE Server_Plugin SHALL skip the copy operation.
3. WHEN the UI extension files exist but the installed version is older than the plugin version, THE Server_Plugin SHALL overwrite the existing files with the updated versions.
4. IF the target extensions directory does not exist, THEN THE Server_Plugin SHALL create it recursively before copying files.
5. IF the file copy operation fails due to a permissions error, THEN THE Server_Plugin SHALL log a descriptive error with the target path and continue initialization without the UI extension.

### Requirement 5: Plugin API Routes

**User Story:** As a frontend developer, I want the server plugin to expose status endpoints, so that the UI can check whether the sidecar is running and display connection state.

#### Acceptance Criteria

1. THE Server_Plugin SHALL register a `GET /status` route on the Plugin_Router that returns a JSON object with fields: `sidecar_running` (boolean), `sidecar_version` (string or null), and `plugin_version` (string).
2. WHEN the Sidecar is running and healthy, THE Server_Plugin SHALL report `sidecar_running: true` and include the Sidecar version string.
3. WHEN the Sidecar is not running, THE Server_Plugin SHALL report `sidecar_running: false` and `sidecar_version: null`.
4. THE Server_Plugin SHALL register a `POST /restart` route on the Plugin_Router that terminates the current Sidecar process and spawns a new one, returning the new health status.

### Requirement 6: Configuration and Logging

**User Story:** As an advanced user, I want to configure plugin behavior through environment variables or a config file, so that I can customize the sidecar port or disable auto-updates.

#### Acceptance Criteria

1. THE Server_Plugin SHALL read configuration from a `config.json` file in the plugin directory if one exists, falling back to default values otherwise.
2. WHERE the `sidecarPort` option is configured, THE Server_Plugin SHALL use the specified port instead of the default 7842 for spawning and health-checking the Sidecar.
3. WHERE the `autoUpdate` option is set to `false`, THE Server_Plugin SHALL skip the version check and download step during initialization.
4. THE Server_Plugin SHALL prefix all log messages with `[ENI]` to distinguish them from other SillyTavern log output.
5. WHEN a critical error occurs during initialization, THE Server_Plugin SHALL log the error at the `error` level and continue running in a degraded state rather than crashing SillyTavern.

### Requirement 7: Cross-Platform Compatibility

**User Story:** As a user on any supported platform, I want the plugin to handle platform-specific differences transparently, so that installation works the same way regardless of my operating system.

#### Acceptance Criteria

1. WHEN running on Windows, THE Server_Plugin SHALL use `taskkill` or process termination APIs instead of SIGTERM/SIGKILL for Sidecar shutdown.
2. WHEN running on macOS or Linux, THE Server_Plugin SHALL set the executable permission bit (0o755) on the downloaded Sidecar binary.
3. THE Server_Plugin SHALL use `path.join()` for all file path construction to ensure correct path separators on each platform.
4. WHEN running on Windows, THE Server_Plugin SHALL append `.exe` to the Sidecar binary filename.
