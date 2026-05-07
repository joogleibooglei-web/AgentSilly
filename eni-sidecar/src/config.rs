use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::info;

/// Top-level application configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// Port for the WebSocket and HTTP server.
    #[serde(default = "default_port")]
    pub listen_port: u16,

    /// Path to the SQLite database file.
    #[serde(default = "default_db_path")]
    pub db_path: String,

    /// Maximum agent loop iterations before halting.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// SillyTavern connection settings.
    #[serde(default)]
    pub sillytavern: StConfig,

    /// Model profiles for LLM access.
    #[serde(default)]
    pub models: Vec<ModelProfile>,
}

/// SillyTavern REST API connection configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StConfig {
    /// SillyTavern REST API base URL.
    #[serde(default = "default_st_url")]
    pub base_url: String,

    /// Optional API key for SillyTavern (if auth is enabled).
    pub api_key: Option<String>,
}

/// A named LLM model profile.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelProfile {
    /// Human-readable name for this profile (e.g., "fast", "creative").
    pub name: String,

    /// OpenAI-compatible API base URL.
    pub base_url: String,

    /// API key for authentication.
    pub api_key: String,

    /// Model identifier (e.g., "gpt-4o", "claude-3-opus").
    pub model: String,

    /// Sampling temperature.
    #[serde(default = "default_temperature")]
    pub temperature: f64,

    /// Maximum tokens for completion.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Whether this is the default profile used when none is explicitly selected.
    #[serde(default)]
    pub is_default: bool,
}

// --- Defaults ---

fn default_port() -> u16 {
    7842
}

fn default_db_path() -> String {
    "eni-sidecar.db".to_string()
}

fn default_st_url() -> String {
    "http://localhost:8000".to_string()
}

fn default_temperature() -> f64 {
    0.7
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_max_iterations() -> u32 {
    15
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen_port: default_port(),
            db_path: default_db_path(),
            max_iterations: default_max_iterations(),
            sillytavern: StConfig::default(),
            models: Vec::new(),
        }
    }
}

impl Default for StConfig {
    fn default() -> Self {
        Self {
            base_url: default_st_url(),
            api_key: None,
        }
    }
}

impl AppConfig {
    /// Returns the default model profile, or the first one if none is marked default.
    /// Returns `None` if no model profiles are configured.
    pub fn default_model(&self) -> Option<&ModelProfile> {
        self.models
            .iter()
            .find(|m| m.is_default)
            .or_else(|| self.models.first())
    }

    /// Find a model profile by name.
    pub fn model_by_name(&self, name: &str) -> Option<&ModelProfile> {
        self.models.iter().find(|m| m.name == name)
    }
}

/// Load configuration from a TOML config file.
///
/// Resolution order:
/// 1. Explicit path passed as argument (from CLI `--config` flag)
/// 2. Path specified by `ENI_CONFIG` environment variable
/// 3. `~/.config/eni-sidecar/config.toml`
/// 4. Falls back to default configuration
pub fn load_config() -> Result<AppConfig> {
    // Check CLI argument: --config <path>
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--config") {
        if let Some(path) = args.get(pos + 1) {
            info!(path = %path, "Loading config from --config argument");
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path))?;
            let config: AppConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", path))?;
            return Ok(config);
        }
    }

    // Check for env var override
    if let Ok(path) = std::env::var("ENI_CONFIG") {
        if std::path::Path::new(&path).exists() {
            info!(path = %path, "Loading config from ENI_CONFIG");
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {}", path))?;
            let config: AppConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", path))?;
            return Ok(config);
        }
    }

    // Check default config path: ~/.config/eni-sidecar/config.toml
    if let Some(home) = home_dir() {
        let config_path = home.join(".config/eni-sidecar/config.toml");
        if config_path.exists() {
            info!(path = %config_path.display(), "Loading config from default path");
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
            let config: AppConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;
            return Ok(config);
        }
    }

    // Fall back to defaults
    info!("No config file found, using defaults");
    Ok(AppConfig::default())
}

/// Get the user's home directory.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.listen_port, 7842);
        assert_eq!(config.db_path, "eni-sidecar.db");
        assert_eq!(config.max_iterations, 15);
        assert_eq!(config.sillytavern.base_url, "http://localhost:8000");
        assert!(config.models.is_empty());
        assert!(config.default_model().is_none());
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
listen_port = 9000
db_path = "/tmp/eni.db"
max_iterations = 20

[sillytavern]
base_url = "http://localhost:8080"
api_key = "st-secret"

[[models]]
name = "fast"
base_url = "https://api.openai.com/v1"
api_key = "sk-test"
model = "gpt-4o-mini"
temperature = 0.5
max_tokens = 2048
is_default = true

[[models]]
name = "creative"
base_url = "https://api.anthropic.com/v1"
api_key = "sk-ant-test"
model = "claude-3-opus"
temperature = 0.9
max_tokens = 8192
is_default = false
"#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.listen_port, 9000);
        assert_eq!(config.db_path, "/tmp/eni.db");
        assert_eq!(config.max_iterations, 20);
        assert_eq!(config.sillytavern.base_url, "http://localhost:8080");
        assert_eq!(config.sillytavern.api_key.as_deref(), Some("st-secret"));
        assert_eq!(config.models.len(), 2);

        let default = config.default_model().unwrap();
        assert_eq!(default.name, "fast");
        assert!(default.is_default);

        let creative = config.model_by_name("creative").unwrap();
        assert_eq!(creative.model, "claude-3-opus");
        assert_eq!(creative.temperature, 0.9);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = r#"
[[models]]
name = "default"
base_url = "https://api.openai.com/v1"
api_key = "sk-test"
model = "gpt-4o"
"#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.listen_port, 7842); // default
        assert_eq!(config.models.len(), 1);
        // First model is returned as default when none marked
        assert!(config.default_model().is_some());
    }
}
