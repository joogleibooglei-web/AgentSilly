<p align="center">
  <img src="https://img.shields.io/badge/SillyTavern-Server%20Plugin-blueviolet?style=for-the-badge" alt="SillyTavern Server Plugin" />
  <img src="https://img.shields.io/badge/plugin-0.5.8-blue?style=for-the-badge" alt="Plugin Version 0.5.8" />
  <img src="https://img.shields.io/badge/sidecar-0.5.8-orange?style=for-the-badge&logo=rust" alt="Sidecar 0.5.8" />
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
- 📚 **Character Management** — Create, read, update, and export character cards with full TavernCard V2 spec support (alternate greetings, character books, extensions, talkativeness)
- 👤 **Persona Support** — Create, read, update, and list user personas with dedicated formatting rules
- 🌐 **World Entries** — Manage lorebook entries with semantic search and intelligent suggestions
- 📝 **Draft Workflow** — Stage edits to world info and post-history instructions via create/edit/read/finalize drafts before committing changes
- 📖 **Lorebook-Style Prompt Extensions** — Keyword-triggered context injection that loads formatting instructions on-demand (character cards, personas, world info, post-history blocks)
- 📋 **Project & Task Management** — Organize world building work into projects with trackable tasks
- 🔍 **Full-Text Search** — BM25-powered search across world entries, characters, and document chunks via tantivy
- 🌐 **Wiki Integration** — Search and fetch wiki pages for research and reference material
- ⚡ **Rust Sidecar** — High-performance backend for fast search indexing and AI inference
- 🔄 **Auto-Connect** — Automatically detects SillyTavern on any common port (8000, 8080, 8181, 8787, 8888, 5000, 5001, 3000) with self-healing reconnection
- 🔄 **Auto-Updates** — Sidecar binary updates automatically from GitHub Releases
- 💥 **Crash Recovery** — Automatic restart with rate limiting if the sidecar goes down
- ↩️ **Version History & Undo** — Entity versioning with undo support for characters and world entries
- 🔌 **Export** — Export character cards in standard format for sharing

---

## 🔌 Connection Architecture

The plugin manages two separate connections:

| Connection | Protocol | Purpose |
|-----------|----------|---------|
| **Sidecar** | WebSocket (port 7842) | Frontend ↔ Rust agent (chat, tools, streaming) |
| **SillyTavern Server** | HTTP REST | Sidecar ↔ ST API (character CRUD, personas, exports) |

The SillyTavern URL is auto-detected from the browser context (`window.location.origin`) and reported to the sidecar on connect. You can also set it manually in Settings → SillyTavern Server.

If ST restarts or moves ports, the sidecar automatically re-detects it on the next request.

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

### Sidecar Configuration

The sidecar reads `~/.config/eni-sidecar/config.toml`:

```toml
listen_port = 7842
http_port = 7843

[sillytavern]
base_url = "http://localhost:8000"
# api_key = "your-st-api-key"  # if ST auth is enabled

[[models]]
name = "default"
base_url = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4o"
temperature = 0.7
max_tokens = 4096
is_default = true
```

Config resolution order:
1. CLI `--config <path>`
2. `ENI_CONFIG` environment variable
3. `~/.config/eni-sidecar/config.toml`
4. Built-in defaults

> **Note:** The ST URL in the TOML is a fallback. The frontend auto-reports the correct URL on connect, which takes priority.

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
  "sidecar_version": "0.5.8",
  "plugin_version": "0.5.8"
}
```

---

## 🏗️ Architecture

```
plugins/eni-world-builder/
├── index.js              ← Server Plugin entry point (init/exit)
├── plugin.json           ← ST plugin manifest
├── manifest.json         ← UI extension manifest
├── install.sh            ← One-line installer script
├── config.json           ← Optional user configuration (gitignored)
├── dist/                 ← Built frontend assets (Svelte)
├── bin/                  ← Downloaded sidecar binary (gitignored)
├── lib/
│   ├── config.js         ← Configuration manager
│   ├── binary.js         ← Binary download & version management
│   ├── sidecar.js        ← Process lifecycle (spawn/kill/crash recovery)
│   ├── ui-installer.js   ← UI extension auto-installer
│   └── routes.js         ← API route registration
├── frontend/             ← Svelte 5 source (dev only)
│   └── src/
│       ├── components/
│       │   ├── ChatPane.svelte
│       │   ├── MessageBubble.svelte
│       │   ├── ThinkingBlock.svelte
│       │   ├── ToolCallCard.svelte
│       │   ├── ModelSelector.svelte
│       │   ├── UndoToast.svelte
│       │   ├── SetupGuide.svelte
│       │   ├── PanelShell.svelte
│       │   ├── RightPane.svelte
│       │   └── tabs/
│       │       ├── CharacterTab.svelte
│       │       ├── PersonaTab.svelte
│       │       ├── PostHistoryTab.svelte
│       │       ├── SettingsTab.svelte
│       │       └── WorldTab.svelte
│       └── lib/
│           ├── stores/   ← Svelte stores
│           └── ws/       ← WebSocket client
└── eni-sidecar/          ← Rust sidecar source (dev only)
    └── src/
        ├── main.rs       ← Entry point, server startup
        ├── config.rs     ← TOML config loading
        ├── prompts.rs    ← ENI personality system prompt
        ├── agent/        ← Agent loop (LLM call → tool exec → stream)
        ├── context/      ← Prompt assembly & token budgeting
        ├── db/           ← SQLite database & schema
        ├── http/         ← HTTP REST API (health, search)
        ├── llm/          ← OpenAI-compatible LLM client
        ├── lorebook/     ← Keyword-triggered prompt extensions
        ├── search/       ← BM25 full-text search (tantivy)
        ├── tools/        ← Tool implementations
        ├── versioning/   ← Entity version history & undo
        └── ws/           ← WebSocket server & message dispatch
```

### Agent Tools

| Tool | Description |
|------|-------------|
| `read_character` | Read fields from a character card |
| `update_character` | Update fields on an existing character |
| `create_character` | Create a new character card |
| `list_characters` | List all available characters |
| `export_card` | Export a character card |
| `read_persona` | Read the active user persona |
| `update_persona` | Update an existing persona |
| `create_persona` | Create a new persona |
| `list_personas` | List all personas |
| `create_world_draft` | Stage a new world info entry |
| `edit_world_draft` | Edit a staged world info draft |
| `read_world_draft` | Read a staged world info draft |
| `finalize_world_info` | Commit a world info draft |
| `create_post_history_draft` | Stage a new post-history instruction |
| `edit_post_history_draft` | Edit a staged post-history draft |
| `read_post_history_draft` | Read a staged post-history draft |
| `finalize_post_history` | Commit a post-history draft |
| `search_local` | BM25 full-text search across indexed content |
| `search_wiki` | Search wiki pages for reference material |
| `fetch_wiki_page` | Fetch full content of a wiki page |
| `create_project` | Create a world building project |
| `manage_tasks` | Create and manage tasks within a project |
| `list_versions` | List version history for an entity |
| `undo_change` | Revert an entity to a previous version |

### How It Works

```
SillyTavern starts
  └─ Loads plugin, calls init(router)
       ├─ Downloads/updates sidecar binary from GitHub
       ├─ Spawns sidecar process on configured port
       ├─ Monitors health endpoint (http://127.0.0.1:7843/health)
       ├─ Enables crash recovery (max 5 restarts per 60s)
       ├─ Copies UI extension to ST extensions directory
       └─ Registers /status and /restart API routes

Frontend connects to sidecar (WebSocket)
  └─ Reports ST URL (window.location.origin)
       └─ Sidecar stores URL, uses it for all ST API calls

User sends message
  └─ Lorebook scans for keywords (last 3 messages)
       ├─ Matches? → Injects formatting instructions into context
       └─ Agent loop: LLM call → tool execution → stream response
            (max 15 iterations to prevent infinite loops)

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

### Frontend (Svelte 5 + Vite 6)

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
cargo test
```

### Running Locally

1. Start SillyTavern with `enableServerPlugins: true`
2. Build the frontend: `cd frontend && npm run build`
3. Build the sidecar: `cd eni-sidecar && cargo build --release`
4. Place the sidecar binary in `bin/` (or let the plugin download it)
5. Restart SillyTavern — the plugin will spawn the sidecar and install the UI

### CI/CD

GitHub Actions builds cross-platform release binaries on tag push (`v*`). Binaries are attached to GitHub Releases for automatic download by the plugin's auto-update system.

---

## 📄 License

MIT

---

<p align="center">
  <sub>Built with ❤️ for the SillyTavern community</sub>
</p>
