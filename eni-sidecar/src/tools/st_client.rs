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
    /// Alternate greetings (additional first messages the user can swap between).
    #[serde(default)]
    pub alternate_greetings: Vec<String>,
    /// Embedded lorebook / character book (world info entries bundled with the card).
    #[serde(default)]
    pub character_book: Option<serde_json::Value>,
    /// Freeform extension data (depth prompts, ST plugins, etc.).
    #[serde(default)]
    pub extensions: Option<serde_json::Value>,
    /// Card creator attribution.
    #[serde(default)]
    pub creator: String,
    /// Version string for the card.
    #[serde(default)]
    pub character_version: String,
    /// Talkativeness (0.0–1.0) — how often the character initiates in group chats.
    #[serde(default)]
    pub talkativeness: Option<f64>,
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
    ///
    /// Detection strategy:
    /// 1. Try the user-configured URL first (highest priority — user knows best)
    /// 2. Probe common SillyTavern ports on localhost
    /// 3. Probe common SillyTavern ports on 127.0.0.1 (in case localhost resolves to IPv6)
    /// 4. Fall back to the configured URL (will fail gracefully on first real request)
    ///
    /// Each probe validates that the response is actually SillyTavern by checking
    /// for the CSRF token endpoint's expected response shape.
    async fn detect_st_url(http: &reqwest::Client, configured_url: &str, api_key: Option<&str>) -> String {
        // Try configured URL first — always respect explicit user config
        if Self::probe_st(http, configured_url, api_key).await {
            info!(url = %configured_url, "SillyTavern found at configured URL");
            return configured_url.to_string();
        }

        debug!(url = %configured_url, "SillyTavern not found at configured URL, scanning common ports...");

        // Common SillyTavern ports in order of likelihood:
        // 8000 — ST default
        // 8080 — common alternative / Docker
        // 8181 — ST alternative config
        // 8787 — ST alternative config
        // 8888 — ST alternative config
        // 5000 — older ST versions / some Docker setups
        // 5001 — ST with SSL on some setups
        // 3000 — occasionally used in dev setups
        let common_ports: &[u16] = &[8000, 8080, 8181, 8787, 8888, 5000, 5001, 3000];

        // Probe localhost first (covers both IPv4 and IPv6 depending on OS resolution)
        for &port in common_ports {
            let candidate = format!("http://localhost:{}", port);
            if candidate == configured_url {
                continue; // Already tried
            }
            if Self::probe_st(http, &candidate, api_key).await {
                info!(url = %candidate, "SillyTavern auto-detected on localhost");
                return candidate;
            }
        }

        // Probe 127.0.0.1 explicitly (in case localhost resolves to ::1 and ST only binds IPv4)
        for &port in common_ports {
            let candidate = format!("http://127.0.0.1:{}", port);
            if candidate == configured_url {
                continue;
            }
            if Self::probe_st(http, &candidate, api_key).await {
                info!(url = %candidate, "SillyTavern auto-detected on 127.0.0.1");
                return candidate;
            }
        }

        // Fall back to configured URL — will fail gracefully on first real request
        warn!("SillyTavern not detected on any common port. Using configured URL: {}", configured_url);
        configured_url.to_string()
    }

    /// Probe a URL to check if SillyTavern is running there.
    ///
    /// Validates the response is actually SillyTavern by checking that the
    /// `/csrf-token` endpoint returns a JSON object with a `token` or `csrf` field,
    /// or a non-empty text body (older ST versions return plain text tokens).
    async fn probe_st(http: &reqwest::Client, url: &str, api_key: Option<&str>) -> bool {
        let probe_url = format!("{}/csrf-token", url);
        let mut req = http.get(&probe_url).timeout(std::time::Duration::from_secs(2));
        if let Some(key) = api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return false;
                }
                // Validate this is actually SillyTavern by checking response body
                match resp.text().await {
                    Ok(body) => {
                        // ST returns either {"token": "..."} or plain text token
                        if body.is_empty() {
                            return false;
                        }
                        // JSON response with token field = definitely ST
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                            return json.get("token").is_some() || json.get("csrf").is_some();
                        }
                        // Plain text response that looks like a token (non-empty, no HTML)
                        let trimmed = body.trim();
                        !trimmed.is_empty() && !trimmed.starts_with('<')
                    }
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }

    /// Attempt to reconnect to SillyTavern.
    ///
    /// Re-runs the detection logic and refreshes the CSRF token.
    /// Call this when a request fails due to connection issues.
    pub async fn reconnect(&mut self, config: &StConfig) -> Result<()> {
        let configured_url = config.base_url.trim_end_matches('/').to_string();

        info!("Attempting to reconnect to SillyTavern...");

        let new_url = Self::detect_st_url(&self.http, &configured_url, config.api_key.as_deref()).await;

        if new_url != self.base_url {
            info!(old = %self.base_url, new = %new_url, "SillyTavern URL changed");
        }

        self.base_url = new_url;
        self.api_key = config.api_key.clone();
        self.csrf_token = None;

        self.fetch_csrf_token().await?;
        info!(url = %self.base_url, "Reconnected to SillyTavern");
        Ok(())
    }

    /// Check if the client currently has a valid connection (CSRF token acquired).
    pub fn is_connected(&self) -> bool {
        self.csrf_token.is_some()
    }

    /// Get the currently detected base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
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
    ///
    /// If the token is missing (either never acquired or invalidated after an error),
    /// this re-runs detection to handle cases where ST started after the sidecar,
    /// or moved to a different port.
    async fn ensure_csrf(&mut self) -> Result<()> {
        if self.csrf_token.is_none() {
            // Re-detect ST URL in case it moved or just started
            let current_url = self.base_url.clone();
            let new_url = Self::detect_st_url(
                &self.http,
                &current_url,
                self.api_key.as_deref(),
            ).await;

            if new_url != current_url {
                info!(old = %current_url, new = %new_url, "SillyTavern URL updated during reconnect");
                self.base_url = new_url;
            }

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

    /// Invalidate the CSRF token, forcing re-detection on next request.
    ///
    /// Call this when a request fails due to connection issues so the next
    /// `ensure_csrf()` call will re-probe for SillyTavern.
    pub fn invalidate_connection(&mut self) {
        debug!("Invalidating CSRF token — will re-detect ST on next request");
        self.csrf_token = None;
    }

    /// List all characters available in SillyTavern.
    ///
    /// Returns a summary for each character (name, avatar, last_modified).
    /// On connection failure, invalidates the session so the next call re-detects ST.
    pub async fn get_characters(&mut self) -> Result<Vec<CharacterSummary>> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/all", self.base_url);
        debug!(url = %url, "Fetching character list");

        let resp = self.get_request(&url).send().await.map_err(|e| {
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            // 403 often means CSRF token expired — invalidate so we re-fetch
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
            self.invalidate_connection();
            anyhow::anyhow!(
                "SillyTavern is not reachable at '{}': {}. \
                 Please ensure SillyTavern is running and the base URL is correct.",
                self.base_url,
                e
            )
        })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
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
