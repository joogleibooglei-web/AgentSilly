//! SillyTavern REST API client.
//!
//! Provides an HTTP client for interacting with SillyTavern's character management
//! endpoints. Handles CSRF token fetching and API key authentication.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::config::StConfig;

/// Summary of a character returned by the list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSummary {
    /// Character display name.
    pub name: String,
    /// Avatar filename or identifier.
    #[serde(default)]
    pub avatar: String,
    /// Last modification timestamp (ISO 8601 or unix millis depending on ST version).
    #[serde(default)]
    pub last_modified: Option<String>,
}

/// Full character card data (TavernCard V2 fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterData {
    /// Character name (required).
    pub name: String,
    /// Character description / backstory.
    #[serde(default)]
    pub description: String,
    /// Personality summary.
    #[serde(default)]
    pub personality: String,
    /// Scenario / setting context.
    #[serde(default)]
    pub scenario: String,
    /// First message the character sends.
    #[serde(default)]
    pub first_mes: String,
    /// Example dialogue (mes_example).
    #[serde(default)]
    pub mes_example: String,
    /// Creator notes (metadata).
    #[serde(default)]
    pub creator_notes: String,
    /// System prompt override for this character.
    #[serde(default)]
    pub system_prompt: String,
    /// Post-history instructions for this character.
    #[serde(default)]
    pub post_history_instructions: String,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Avatar filename.
    #[serde(default)]
    pub avatar: String,
}

/// Summary of a persona returned by the list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSummary {
    /// Persona display name.
    pub name: String,
    /// Avatar filename or identifier.
    #[serde(default)]
    pub avatar: String,
}

/// Full persona data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaData {
    /// Persona name (required).
    pub name: String,
    /// Persona description / content.
    #[serde(default)]
    pub description: String,
    /// Avatar filename.
    #[serde(default)]
    pub avatar: String,
}

/// HTTP client for the SillyTavern REST API.
///
/// Manages CSRF token acquisition and includes it in all mutating requests.
/// If an API key is configured, it is sent via the `Authorization` header.
pub struct StClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    csrf_token: Option<String>,
}

impl StClient {
    /// Create a new ST client and attempt to fetch the CSRF token.
    ///
    /// If SillyTavern is unreachable during construction, the client is still
    /// created but the CSRF token will be `None`. Methods will attempt to
    /// re-fetch the token on first use if missing.
    pub async fn new(config: &StConfig) -> Result<Self> {
        let configured_url = config.base_url.trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        // Try the configured URL first, then probe common ports
        let base_url = Self::detect_st_url(&http, &configured_url, config.api_key.as_deref()).await;

        let mut client = Self {
            http,
            base_url,
            api_key: config.api_key.clone(),
            csrf_token: None,
        };

        // Attempt to fetch CSRF token; log warning if ST is unreachable.
        if let Err(e) = client.fetch_csrf_token().await {
            warn!("Could not fetch CSRF token from SillyTavern: {e}. Will retry on first request.");
        }

        Ok(client)
    }

    /// Auto-detect the SillyTavern URL by probing the configured URL and common ports.
    async fn detect_st_url(http: &reqwest::Client, configured_url: &str, api_key: Option<&str>) -> String {
        // Try configured URL first
        if Self::probe_st(http, configured_url, api_key).await {
            debug!(url = %configured_url, "SillyTavern found at configured URL");
            return configured_url.to_string();
        }

        // Probe common SillyTavern ports
        let common_ports = [8000, 8080, 8181, 8787, 8888];
        for port in common_ports {
            let candidate = format!("http://localhost:{}", port);
            if candidate == configured_url {
                continue; // Already tried
            }
            if Self::probe_st(http, &candidate, api_key).await {
                info!(url = %candidate, "SillyTavern auto-detected");
                return candidate;
            }
        }

        // Fall back to configured URL
        debug!("SillyTavern not detected on any common port, using configured URL");
        configured_url.to_string()
    }

    /// Probe a URL to check if SillyTavern is running there.
    async fn probe_st(http: &reqwest::Client, url: &str, api_key: Option<&str>) -> bool {
        let probe_url = format!("{}/csrf-token", url);
        let mut req = http.get(&probe_url).timeout(std::time::Duration::from_secs(2));
        if let Some(key) = api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        match req.send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Fetch the CSRF token from SillyTavern's `/csrf-token` endpoint.
    async fn fetch_csrf_token(&mut self) -> Result<()> {
        let url = format!("{}/csrf-token", self.base_url);
        debug!(url = %url, "Fetching CSRF token");

        let mut req = self.http.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }

        let resp = req.send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Failed to fetch CSRF token: HTTP {}",
                resp.status()
            );
        }

        // ST returns the token as a JSON object: { "token": "..." }
        // or as plain text depending on version. Handle both.
        let body = resp.text().await?;
        let token = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            json.get("token")
                .or_else(|| json.get("csrf"))
                .and_then(|v| v.as_str())
                .unwrap_or(&body)
                .to_string()
        } else {
            body.trim().to_string()
        };

        debug!("CSRF token acquired");
        self.csrf_token = Some(token);
        Ok(())
    }

    /// Ensure we have a CSRF token, fetching one if needed.
    async fn ensure_csrf(&mut self) -> Result<()> {
        if self.csrf_token.is_none() {
            self.fetch_csrf_token().await.map_err(|e| {
                anyhow::anyhow!(
                    "SillyTavern is not reachable at '{}'. \
                     Please ensure SillyTavern is running and the Sidecar Connection URL is correct in Settings. \
                     Error: {}",
                    self.base_url,
                    e
                )
            })?;
        }
        Ok(())
    }

    /// Build a GET request with auth and CSRF headers.
    fn get_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.http.get(url);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        if let Some(ref token) = self.csrf_token {
            req = req.header("X-CSRF-Token", token.as_str());
        }
        req
    }

    /// Build a POST request with auth and CSRF headers.
    fn post_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.http.post(url);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        if let Some(ref token) = self.csrf_token {
            req = req.header("X-CSRF-Token", token.as_str());
        }
        req
    }

    /// List all characters available in SillyTavern.
    ///
    /// Returns a summary for each character (name, avatar, last_modified).
    pub async fn get_characters(&mut self) -> Result<Vec<CharacterSummary>> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/all", self.base_url);
        debug!(url = %url, "Fetching character list");

        let resp = self.get_request(&url).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Failed to list characters: HTTP {} from {}",
                resp.status(),
                url
            );
        }

        let characters: Vec<CharacterSummary> = resp.json().await.context(
            "Failed to parse character list response from SillyTavern",
        )?;

        debug!(count = characters.len(), "Fetched characters");
        Ok(characters)
    }

    /// Get full character data by name.
    pub async fn get_character(&mut self, name: &str) -> Result<CharacterData> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/get", self.base_url);
        debug!(url = %url, name = %name, "Fetching character data");

        let body = serde_json::json!({ "name": name });

        let resp = self.post_request(&url).json(&body).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Failed to get character '{}': HTTP {} from {}",
                name,
                resp.status(),
                url
            );
        }

        let character: CharacterData = resp.json().await.with_context(|| {
            format!("Failed to parse character data for '{name}' from SillyTavern")
        })?;

        Ok(character)
    }

    /// Create a new character in SillyTavern.
    pub async fn create_character(&mut self, data: &CharacterData) -> Result<()> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/create", self.base_url);
        debug!(url = %url, name = %data.name, "Creating character");

        let resp = self.post_request(&url).json(data).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Failed to create character '{}': HTTP {} — {}",
                data.name,
                status,
                body
            );
        }

        debug!(name = %data.name, "Character created");
        Ok(())
    }

    /// Edit an existing character in SillyTavern.
    pub async fn edit_character(&mut self, data: &CharacterData) -> Result<()> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/edit", self.base_url);
        debug!(url = %url, name = %data.name, "Editing character");

        let resp = self.post_request(&url).json(data).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Failed to edit character '{}': HTTP {} — {}",
                data.name,
                status,
                body
            );
        }

        debug!(name = %data.name, "Character edited");
        Ok(())
    }

    /// Delete a character from SillyTavern by name.
    pub async fn delete_character(&mut self, name: &str) -> Result<()> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/delete", self.base_url);
        debug!(url = %url, name = %name, "Deleting character");

        let body = serde_json::json!({ "name": name });

        let resp = self.post_request(&url).json(&body).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Failed to delete character '{}': HTTP {} — {}",
                name,
                status,
                body_text
            );
        }

        debug!(name = %name, "Character deleted");
        Ok(())
    }

    // ─── Persona endpoints ───────────────────────────────────────────────

    /// List all personas available in SillyTavern.
    pub async fn get_personas(&mut self) -> Result<Vec<PersonaSummary>> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/personas/all", self.base_url);
        debug!(url = %url, "Fetching persona list");

        let resp = self.get_request(&url).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Failed to list personas: HTTP {} from {}",
                resp.status(),
                url
            );
        }

        let personas: Vec<PersonaSummary> = resp.json().await.context(
            "Failed to parse persona list response from SillyTavern",
        )?;

        debug!(count = personas.len(), "Fetched personas");
        Ok(personas)
    }

    /// Get full persona data by name.
    pub async fn get_persona(&mut self, name: &str) -> Result<PersonaData> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/personas/get", self.base_url);
        debug!(url = %url, name = %name, "Fetching persona data");

        let body = serde_json::json!({ "name": name });

        let resp = self.post_request(&url).json(&body).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Failed to get persona '{}': HTTP {} from {}",
                name,
                resp.status(),
                url
            );
        }

        let persona: PersonaData = resp.json().await.with_context(|| {
            format!("Failed to parse persona data for '{name}' from SillyTavern")
        })?;

        Ok(persona)
    }

    /// Edit an existing persona in SillyTavern.
    pub async fn edit_persona(&mut self, data: &PersonaData) -> Result<()> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/personas/edit", self.base_url);
        debug!(url = %url, name = %data.name, "Editing persona");

        let resp = self.post_request(&url).json(data).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Failed to edit persona '{}': HTTP {} — {}",
                data.name,
                status,
                body
            );
        }

        debug!(name = %data.name, "Persona edited");
        Ok(())
    }

    // ─── Character export endpoint ──────────────────────────────────────────

    /// Export a character's full JSON data from SillyTavern.
    ///
    /// Returns the raw JSON value representing the complete character export.
    pub async fn export_character(&mut self, name: &str) -> Result<serde_json::Value> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/export", self.base_url);
        debug!(url = %url, name = %name, "Exporting character");

        let body = serde_json::json!({ "name": name });

        let resp = self.post_request(&url).json(&body).send().await.map_err(|e| {
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Failed to export character '{}': HTTP {} — {}",
                name,
                status,
                body_text
            );
        }

        let export: serde_json::Value = resp.json().await.with_context(|| {
            format!("Failed to parse export data for character '{name}' from SillyTavern")
        })?;

        debug!(name = %name, "Character exported");
        Ok(export)
    }
}
