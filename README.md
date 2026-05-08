<p align="center">
  <img src="https://img.shields.io/badge/SillyTavern-Server%20Plugin-blueviolet?style=for-the-badge" alt="SillyTavern Server Plugin" />
  <img src="https://img.shields.io/badge/version-0.2.0-blue?style=for-the-badge" alt="Version 0.2.0" />
  <img src="https://img.shields.io/badge/rust-sidecar-orange?style=for-the-badge&logo=rust" alt="Rust Sidecar" />
  <img src="https://img.shields.io/badge/license-MIT-green?style=for-the-badge" alt="MIT License" />
</p>

<h1 align="center">🌍 ENI World Builder</h1>

<p align="center">
  <strong>An AI-powered world building assistant for SillyTavern</strong><br/>
  <em>Build rich, consistent fictional worlds with intelligent character and lore management.</em>
</p>

<p align="center">
  <a href="#-quick-install">Install</a> •
  <a href="#-features">Features</a> •
  <a href="#%EF%B8%8F-configuration">Configuration</a> •
  <a href="#-architecture">Architecture</a> •
  <a href="#-development">Development</a>
</p>

---

## 🚀 Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/joogleibooglei-web/AgentSilly/main/install.sh | bash
```

That's it. The script will ask for your SillyTavern path, then handle everything else:

- ✅ Checks dependencies (git, Node.js 18+)
- ✅ Detects your platform
- ✅ Enables server plugins in your ST config
- ✅ Clones the plugin into the correct directory
- ✅ Verifies the installation

Then restart SillyTavern. On first launch the plugin automatically downloads the sidecar binary, starts it, and installs the UI extension.

> **One command. No manual binary downloads. No separate UI extension install.**

<details>
<summary><strong>Manual install</strong> (click to expand)</summary>

<br/>

**Prerequisites:** [SillyTavern](https://github.com/SillyTavern/SillyTavern), Node.js 18+, macOS/Linux/Windows (x64 or ARM64 on macOS)

1. Enable server plugins in `config.yaml`:
   ```yaml
   enableServerPlugins: true
   ```

2. Clone into the plugins directory:
   ```bash
   cd /path/to/SillyTavern/plugins
   git clone https://github.com/joogleibooglei-web/AgentSilly.git eni-world-builder
   ```

3. Restart SillyTavern.

</details>

---

## ✨ Features

- 🤖 **AI-Powered Agent** — Intelligent assistant that helps you create, edit, and manage world building content
- 📚 **Character Management** — Create, read, and export character cards with full V2 spec support
- 🌐 **World Entries** — Manage lorebook entries with semantic search and intelligent suggestions
- 🔍 **Semantic Search** — Find relevant lore and characters using natural language queries
- ⚡ **Rust Sidecar** — High-performance backend for fast search indexing and AI inference
- 🔄 **Auto-Updates** — Sidecar binary updates automatically from GitHub Releases
- 💥 **Crash Recovery** — Automatic restart with rate limiting if the sidecar goes down

---

## ⚙️ Configuration

Create a `config.json` in the plugin directory to customize behavior:

```json
{
  "sidecarPort": 7842,
  "autoUpdate": true,
  "dataRoot": null
}
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `sidecarPort` | number | `7842` | Port for the sidecar HTTP/WebSocket server |
| `autoUpdate` | boolean | `true` | Automatically check for and download sidecar updates |
| `dataRoot` | string \| null | `null` | Override ST data root path (auto-detected if null) |

---

## 🔌 API

The plugin registers routes on the SillyTavern plugin router:

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/plugins/eni-world-builder/status` | Returns sidecar status and version info |
| `POST` | `/api/plugins/eni-world-builder/restart` | Restarts the sidecar process |

### `GET /status` Response

```json
{
  "sidecar_running": true,
  "sidecar_version": "0.2.0",
  "plugin_version": "0.2.0"
}
```

---

## 🏗️ Architecture

```
plugins/eni-world-builder/
├── index.js              ← Server Plugin entry point (init/exit)
├── plugin.json           ← ST plugin manifest
├── manifest.json         ← UI extension manifest
├── config.json           ← Optional user configuration
├── dist/                 ← Built frontend assets (Svelte)
├── bin/                  ← Downloaded sidecar binary (gitignored)
├── lib/
│   ├── config.js         ← Configuration manager
│   ├── binary.js         ← Binary download & version management
│   ├── sidecar.js        ← Process lifecycle (spawn/kill/crash recovery)
│   ├── ui-installer.js   ← UI extension auto-installer
│   └── routes.js         ← API route registration
├── frontend/             ← Svelte source (dev only)
└── eni-sidecar/          ← Rust sidecar source (dev only)
```

### How It Works

```
SillyTavern starts
  └─ Loads plugin, calls init(router)
       ├─ Downloads/updates sidecar binary from GitHub
       ├─ Spawns sidecar process on configured port
       ├─ Monitors health endpoint (http://127.0.0.1:7842/health)
       ├─ Enables crash recovery (max 5 restarts per 60s)
       ├─ Copies UI extension to ST extensions directory
       └─ Registers /status and /restart API routes

SillyTavern shuts down
  └─ Calls exit()
       ├─ Disables crash recovery
       └─ Sends SIGTERM → waits 5s → SIGKILL
```

---

## 🖥️ Supported Platforms

| Platform | Architecture | Binary Name |
|----------|-------------|-------------|
| macOS | ARM64 (Apple Silicon) | `eni-sidecar-darwin-arm64` |
| macOS | x64 (Intel) | `eni-sidecar-darwin-x64` |
| Linux | x64 | `eni-sidecar-linux-x64` |
| Windows | x64 | `eni-sidecar-win32-x64.exe` |

---

## 🛠️ Development

### Frontend (Svelte)

```bash
cd frontend
npm install
npm run dev     # Watch mode
npm run build   # Production build → dist/
```

### Sidecar (Rust)

```bash
cd eni-sidecar
cargo build --release
```

### Running Tests

```bash
# All tests
node lib/binary.test.js
node lib/binary.platform.test.js
node lib/binary.version.test.js
node lib/config.test.js
node lib/sidecar.test.js
node lib/sidecar.ratelimit.test.js
node lib/ui-installer.test.js
node --test lib/routes.test.js
node lib/lifecycle.test.js
```

---

## 📄 License

MIT

---

<p align="center">
  <sub>Built with ❤️ for the SillyTavern community</sub>
</p>
