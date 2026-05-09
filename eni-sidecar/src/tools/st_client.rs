//! SillyTavern REST API client.
//!
//! Provides an HTTP client for interacting with SillyTavern's character management
//! endpoints. Handles CSRF token fetching and API key authentication.

use anyhow::{Context, Result};
use chrono::Utc;
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
///
/// Uses permissive deserialization to handle cards with missing, null, or
/// differently-typed fields. SillyTavern cards vary widely in format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterData {
    /// Character name (required).
    #[serde(default)]
    pub name: String,
    /// Character description / backstory.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub description: String,
    /// Personality summary.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub personality: String,
    /// Scenario / setting context.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub scenario: String,
    /// First message the character sends.
    #[serde(default, alias = "first_message", alias = "greeting", deserialize_with = "deserialize_string_permissive")]
    pub first_mes: String,
    /// Example dialogue (mes_example).
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub mes_example: String,
    /// Creator notes (metadata).
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub creator_notes: String,
    /// System prompt override for this character.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub system_prompt: String,
    /// Post-history instructions for this character.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub post_history_instructions: String,
    /// Tags for categorization.
    #[serde(default, deserialize_with = "deserialize_vec_string_permissive")]
    pub tags: Vec<String>,
    /// Avatar filename.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub avatar: String,
    /// Alternate greetings (additional first messages the user can swap between).
    #[serde(default, deserialize_with = "deserialize_vec_string_permissive")]
    pub alternate_greetings: Vec<String>,
    /// Embedded lorebook / character book (world info entries bundled with the card).
    #[serde(default)]
    pub character_book: Option<serde_json::Value>,
    /// Freeform extension data (depth prompts, ST plugins, etc.).
    #[serde(default)]
    pub extensions: Option<serde_json::Value>,
    /// Card creator attribution.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub creator: String,
    /// Version string for the card.
    #[serde(default, deserialize_with = "deserialize_string_permissive")]
    pub character_version: String,
    /// Talkativeness (0.0–1.0) — how often the character initiates in group chats.
    #[serde(default, deserialize_with = "deserialize_option_f64_permissive")]
    pub talkativeness: Option<f64>,
}

/// Permissive string deserializer: handles null, numbers, booleans, and missing values.
fn deserialize_string_permissive<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(String::new()),
        Some(serde_json::Value::String(s)) => Ok(s),
        Some(serde_json::Value::Number(n)) => Ok(n.to_string()),
        Some(serde_json::Value::Bool(b)) => Ok(b.to_string()),
        Some(other) => Ok(other.to_string()),
    }
}

/// Permissive Vec<String> deserializer: handles null, non-array values gracefully.
fn deserialize_vec_string_permissive<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(arr)) => {
            Ok(arr.into_iter().filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Null => None,
                other => Some(other.to_string()),
            }).collect())
        }
        Some(serde_json::Value::String(s)) => {
            // Single string — wrap in a vec
            if s.is_empty() { Ok(Vec::new()) } else { Ok(vec![s]) }
        }
        Some(_) => Ok(Vec::new()),
    }
}

/// Permissive Option<f64> deserializer: handles string numbers, null, etc.
fn deserialize_option_f64_permissive<'de, D>(deserializer: D) -> std::result::Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_f64()),
        Some(serde_json::Value::String(s)) => Ok(s.parse::<f64>().ok()),
        Some(_) => Ok(None),
    }
}

/// Summary of a persona returned by the list operation.
///
/// In SillyTavern, personas are stored in `power_user.personas` (a map of
/// avatar_id → name) and `power_user.persona_descriptions` (a map of
/// avatar_id → { description, title, position, depth, role, lorebook }).
/// The avatar list comes from `/api/avatars/get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSummary {
    /// Persona display name.
    pub name: String,
    /// Avatar filename / identifier (the key used to look up persona data).
    #[serde(default)]
    pub avatar: String,
    /// Optional title to differentiate personas with the same name.
    #[serde(default)]
    pub title: String,
}

/// Full persona data.
///
/// Assembled from SillyTavern's settings: `power_user.personas[avatar_id]` for
/// the name, and `power_user.persona_descriptions[avatar_id]` for description,
/// title, position, depth, and role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaData {
    /// Persona name (required).
    pub name: String,
    /// Persona description / content.
    #[serde(default)]
    pub description: String,
    /// Avatar filename / identifier (the key in power_user maps).
    #[serde(default)]
    pub avatar: String,
    /// Optional title to differentiate personas with the same name.
    #[serde(default)]
    pub title: String,
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
            .cookie_store(true)
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
    /// Returns a lightweight summary for each character (name + avatar filename).
    /// The ST `/api/characters/all` endpoint returns full card data, but we only
    /// extract the fields needed for listing to avoid massive token counts.
    pub async fn get_characters(&mut self) -> Result<Vec<CharacterSummary>> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/all", self.base_url);
        debug!(url = %url, "Fetching character list");

        // ST requires POST for this endpoint (not GET)
        let resp = self.post_request(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| {
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
                "Failed to list characters: HTTP {} from {}",
                resp.status(),
                url
            );
        }

        // ST returns full character objects — extract only name + avatar
        let raw: Vec<serde_json::Value> = resp.json().await.context(
            "Failed to parse character list response from SillyTavern",
        )?;

        let characters: Vec<CharacterSummary> = raw
            .iter()
            .filter_map(|c| {
                let name = c.get("name").and_then(|v| v.as_str())?.to_string();
                let avatar = c.get("avatar").and_then(|v| v.as_str()).unwrap_or("").to_string();
                Some(CharacterSummary {
                    name,
                    avatar,
                    last_modified: None,
                })
            })
            .collect();

        debug!(count = characters.len(), "Fetched characters");
        Ok(characters)
    }

    /// Get full character data by avatar filename.
    ///
    /// ST uses `avatar_url` (the PNG filename, e.g. "Akko.png") as the character
    /// identifier, not the character name.
    pub async fn get_character(&mut self, avatar_url: &str) -> Result<CharacterData> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/get", self.base_url);
        debug!(url = %url, avatar_url = %avatar_url, "Fetching character data");

        let body = serde_json::json!({ "avatar_url": avatar_url });

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
                avatar_url,
                resp.status(),
                url
            );
        }

        let raw: serde_json::Value = resp.json().await.with_context(|| {
            format!("Failed to parse character data for '{avatar_url}' from SillyTavern")
        })?;

        // SillyTavern returns a V2 card with dual structure:
        //   - Top-level V1 fields (name, description, personality, etc.)
        //   - Nested `data` object with V2 fields (data.alternate_greetings, data.creator, etc.)
        //
        // The `readFromV2` function in ST only hoists a subset of fields to the top level.
        // Fields like alternate_greetings, creator_notes, system_prompt, post_history_instructions,
        // creator, and character_version remain ONLY in `data.*`.
        //
        // We deserialize from the top level first, then backfill from `data.*` for any
        // fields that are empty/missing at the top level but present in the nested object.
        let mut character: CharacterData = serde_json::from_value(raw.clone()).with_context(|| {
            format!("Failed to deserialize character data for '{avatar_url}'")
        })?;

        // Backfill fields from `data.*` that ST doesn't hoist to the top level
        if let Some(data_obj) = raw.get("data") {
            if character.alternate_greetings.is_empty() {
                if let Some(alt) = data_obj.get("alternate_greetings").and_then(|v| v.as_array()) {
                    character.alternate_greetings = alt.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                }
            }
            if character.creator_notes.is_empty() {
                if let Some(v) = data_obj.get("creator_notes").and_then(|v| v.as_str()) {
                    character.creator_notes = v.to_string();
                }
            }
            if character.system_prompt.is_empty() {
                if let Some(v) = data_obj.get("system_prompt").and_then(|v| v.as_str()) {
                    character.system_prompt = v.to_string();
                }
            }
            if character.post_history_instructions.is_empty() {
                if let Some(v) = data_obj.get("post_history_instructions").and_then(|v| v.as_str()) {
                    character.post_history_instructions = v.to_string();
                }
            }
            if character.creator.is_empty() {
                if let Some(v) = data_obj.get("creator").and_then(|v| v.as_str()) {
                    character.creator = v.to_string();
                }
            }
            if character.character_version.is_empty() {
                if let Some(v) = data_obj.get("character_version").and_then(|v| v.as_str()) {
                    character.character_version = v.to_string();
                }
            }
            if character.character_book.is_none() {
                character.character_book = data_obj.get("character_book").cloned();
            }
            if character.extensions.is_none() {
                character.extensions = data_obj.get("extensions").cloned();
            }
            if character.tags.is_empty() {
                if let Some(tags) = data_obj.get("tags").and_then(|v| v.as_array()) {
                    character.tags = tags.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                }
            }
        }

        Ok(character)
    }

    /// Create a new character in SillyTavern.
    ///
    /// Uses the create endpoint with form-style JSON body.
    pub async fn create_character(&mut self, data: &CharacterData) -> Result<()> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/create", self.base_url);
        debug!(url = %url, name = %data.name, "Creating character");

        // ST's create endpoint expects ch_name and individual fields
        let body = serde_json::json!({
            "ch_name": data.name,
            "description": data.description,
            "personality": data.personality,
            "scenario": data.scenario,
            "first_mes": data.first_mes,
            "mes_example": data.mes_example,
            "creator_notes": data.creator_notes,
            "system_prompt": data.system_prompt,
            "post_history_instructions": data.post_history_instructions,
            "tags": data.tags,
            "creator": data.creator,
            "character_version": data.character_version,
            "alternate_greetings": data.alternate_greetings,
            "talkativeness": data.talkativeness.unwrap_or(0.5),
        });

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

    /// Edit an existing character in SillyTavern using merge-attributes.
    ///
    /// This does a deep merge of the provided fields into the existing card
    /// and validates against the TavernCard spec. Only changed fields need
    /// to be included in `updates`.
    pub async fn edit_character(&mut self, avatar_url: &str, updates: &serde_json::Value) -> Result<()> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/merge-attributes", self.base_url);
        debug!(url = %url, avatar = %avatar_url, "Editing character via merge-attributes");

        // merge-attributes expects { "avatar": "Name.png", ...fields_to_merge }
        let mut body = updates.clone();
        if let Some(obj) = body.as_object_mut() {
            obj.insert("avatar".to_string(), serde_json::Value::String(avatar_url.to_string()));
        }

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
            let resp_body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Failed to edit character '{}': HTTP {} — {}",
                avatar_url,
                status,
                resp_body
            );
        }

        debug!(avatar = %avatar_url, "Character edited");
        Ok(())
    }

    /// Delete a character from SillyTavern by avatar filename.
    pub async fn delete_character(&mut self, avatar_url: &str) -> Result<()> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/delete", self.base_url);
        debug!(url = %url, avatar_url = %avatar_url, "Deleting character");

        let body = serde_json::json!({ "avatar_url": avatar_url });

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
                avatar_url,
                status,
                body_text
            );
        }

        debug!(avatar_url = %avatar_url, "Character deleted");
        Ok(())
    }

    // ─── Persona endpoints ───────────────────────────────────────────────
    //
    // SillyTavern does NOT have dedicated persona API endpoints.
    // Personas are stored in the user settings (`power_user.personas` and
    // `power_user.persona_descriptions`). The avatar file list comes from
    // `/api/avatars/get`. To read/write persona data we must:
    //   1. GET the avatar list from `/api/avatars/get`
    //   2. GET the settings from `/api/settings/get`
    //   3. Parse `power_user.personas` and `power_user.persona_descriptions`
    //   4. For writes, update the settings and POST to `/api/settings/save`

    /// List all personas available in SillyTavern.
    ///
    /// Fetches the avatar file list and cross-references with settings to get
    /// persona names and titles.
    pub async fn get_personas(&mut self) -> Result<Vec<PersonaSummary>> {
        self.ensure_csrf().await?;

        // 1. Get avatar file list
        let avatars_url = format!("{}/api/avatars/get", self.base_url);
        debug!(url = %avatars_url, "Fetching persona avatar list");

        let resp = self.post_request(&avatars_url)
            .send()
            .await
            .map_err(|e| {
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
                "Failed to list persona avatars: HTTP {} from {}",
                resp.status(),
                avatars_url
            );
        }

        let avatar_list: Vec<String> = resp.json().await.context(
            "Failed to parse avatar list response from SillyTavern",
        )?;

        // 2. Get settings to resolve names and titles
        let settings = self.get_settings_raw().await?;
        let personas_map = settings
            .pointer("/power_user/personas")
            .and_then(|v| v.as_object());
        let descriptions_map = settings
            .pointer("/power_user/persona_descriptions")
            .and_then(|v| v.as_object());

        if personas_map.is_none() {
            warn!("power_user.personas not found in settings. Top-level keys: {:?}",
                settings.as_object().map(|o| o.keys().collect::<Vec<_>>()));
        } else {
            debug!(count = personas_map.unwrap().len(), "Found personas map in settings");
        }

        // 3. Build persona summaries
        let personas: Vec<PersonaSummary> = avatar_list
            .iter()
            .map(|avatar_id| {
                let name = personas_map
                    .and_then(|m| m.get(avatar_id))
                    .and_then(|v| v.as_str())
                    .unwrap_or("[Unnamed Persona]")
                    .to_string();
                let title = descriptions_map
                    .and_then(|m| m.get(avatar_id))
                    .and_then(|v| v.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                PersonaSummary {
                    name,
                    avatar: avatar_id.clone(),
                    title,
                }
            })
            .collect();

        debug!(count = personas.len(), "Fetched personas");
        Ok(personas)
    }

    /// Get full persona data by avatar ID.
    ///
    /// Reads from the settings to get name, description, and title.
    pub async fn get_persona(&mut self, avatar_id: &str) -> Result<PersonaData> {
        self.ensure_csrf().await?;

        let settings = self.get_settings_raw().await?;

        let name = settings
            .pointer("/power_user/personas")
            .and_then(|v| v.get(avatar_id))
            .and_then(|v| v.as_str())
            .unwrap_or("[Unnamed Persona]")
            .to_string();

        let descriptor = settings
            .pointer("/power_user/persona_descriptions")
            .and_then(|v| v.get(avatar_id));

        let description = descriptor
            .and_then(|v| v.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let title = descriptor
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(PersonaData {
            name,
            description,
            avatar: avatar_id.to_string(),
            title,
        })
    }

    /// Find a persona by name (and optionally title) and return its data.
    ///
    /// Since personas are keyed by avatar_id internally, this searches through
    /// all personas to find one matching the given name. If multiple personas
    /// share the same name, the title is used to disambiguate.
    pub async fn find_persona_by_name(&mut self, name: &str, title: Option<&str>) -> Result<PersonaData> {
        let personas = self.get_personas().await?;

        let matches: Vec<&PersonaSummary> = personas
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .collect();

        if matches.is_empty() {
            anyhow::bail!("No persona found with name '{}'", name);
        }

        // If there's a title filter, use it to disambiguate
        let target = if let Some(t) = title {
            matches
                .iter()
                .find(|p| p.title.eq_ignore_ascii_case(t))
                .or(matches.first())
                .unwrap()
        } else {
            matches.first().unwrap()
        };

        self.get_persona(&target.avatar).await
    }

    /// Edit an existing persona in SillyTavern.
    ///
    /// Updates the persona name, description, and/or title in the settings,
    /// then saves the settings back to SillyTavern.
    pub async fn edit_persona(&mut self, data: &PersonaData) -> Result<()> {
        self.ensure_csrf().await?;

        // 1. Get current settings
        let mut settings = self.get_settings_raw().await?;

        // 2. Update persona name in power_user.personas
        if let Some(personas) = settings
            .pointer_mut("/power_user/personas")
            .and_then(|v| v.as_object_mut())
        {
            personas.insert(
                data.avatar.clone(),
                serde_json::Value::String(data.name.clone()),
            );
        }

        // 3. Update persona description and title in power_user.persona_descriptions
        if let Some(descriptions) = settings
            .pointer_mut("/power_user/persona_descriptions")
            .and_then(|v| v.as_object_mut())
        {
            let entry = descriptions
                .entry(data.avatar.clone())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(
                    "description".to_string(),
                    serde_json::Value::String(data.description.clone()),
                );
                obj.insert(
                    "title".to_string(),
                    serde_json::Value::String(data.title.clone()),
                );
            }
        }

        // 4. Save settings back to SillyTavern
        self.save_settings_raw(&settings).await?;

        debug!(avatar = %data.avatar, name = %data.name, "Persona edited");
        Ok(())
    }

    /// Create a new persona in SillyTavern.
    ///
    /// Generates a unique avatar ID and adds the persona name and description
    /// to the settings. The persona will use a default avatar until the user
    /// uploads a custom one through SillyTavern's UI.
    pub async fn create_persona(&mut self, name: &str, description: &str, title: &str) -> Result<String> {
        self.ensure_csrf().await?;

        // Generate a unique avatar ID (timestamp-based, matching ST's pattern)
        let avatar_id = format!("{}.png", Utc::now().timestamp_millis());

        // 1. Upload a default avatar via the upload endpoint
        // ST's /api/avatars/upload expects a JSON body with base64-encoded image data
        let upload_url = format!("{}/api/avatars/upload", self.base_url);
        debug!(url = %upload_url, avatar_id = %avatar_id, "Creating persona avatar");

        // Minimal 1x1 transparent PNG as base64
        let png_base64 = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

        let upload_body = serde_json::json!({
            "avatar": png_base64,
            "overwrite_name": avatar_id
        });

        let resp = self.post_request(&upload_url)
            .json(&upload_body)
            .send()
            .await
            .map_err(|e| {
                self.invalidate_connection();
                anyhow::anyhow!("SillyTavern is not reachable at '{}': {}", self.base_url, e)
            })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            // Non-fatal: persona can still work without avatar upload
            warn!("Avatar upload returned HTTP {} — {}. Proceeding with settings update.", status, body);
        }

        // 2. Update settings to add the persona name and description
        let mut settings = self.get_settings_raw().await?;

        // Add persona name
        if let Some(personas) = settings
            .pointer_mut("/power_user/personas")
            .and_then(|v| v.as_object_mut())
        {
            personas.insert(
                avatar_id.clone(),
                serde_json::Value::String(name.to_string()),
            );
        } else {
            // Create the personas map if it doesn't exist
            if let Some(power_user) = settings.pointer_mut("/power_user").and_then(|v| v.as_object_mut()) {
                let mut personas = serde_json::Map::new();
                personas.insert(avatar_id.clone(), serde_json::Value::String(name.to_string()));
                power_user.insert("personas".to_string(), serde_json::Value::Object(personas));
            }
        }

        // Add persona description and title
        if let Some(descriptions) = settings
            .pointer_mut("/power_user/persona_descriptions")
            .and_then(|v| v.as_object_mut())
        {
            descriptions.insert(
                avatar_id.clone(),
                serde_json::json!({
                    "description": description,
                    "title": title,
                }),
            );
        } else {
            // Create the persona_descriptions map if it doesn't exist
            if let Some(power_user) = settings.pointer_mut("/power_user").and_then(|v| v.as_object_mut()) {
                let mut descriptions = serde_json::Map::new();
                descriptions.insert(avatar_id.clone(), serde_json::json!({
                    "description": description,
                    "title": title,
                }));
                power_user.insert("persona_descriptions".to_string(), serde_json::Value::Object(descriptions));
            }
        }

        // 3. Save settings back
        self.save_settings_raw(&settings).await?;

        debug!(avatar_id = %avatar_id, name = %name, "Persona created");
        Ok(avatar_id)
    }

    /// Fetch the raw settings JSON from SillyTavern.
    ///
    /// SillyTavern's `/api/settings/get` endpoint returns a response object where
    /// the `settings` field is a **JSON string** (the raw content of settings.json),
    /// not a parsed object. This method extracts and parses that inner string to
    /// return the actual user settings as a JSON Value.
    async fn get_settings_raw(&mut self) -> Result<serde_json::Value> {
        let url = format!("{}/api/settings/get", self.base_url);
        debug!(url = %url, "Fetching settings");

        let resp = self.post_request(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| {
                self.invalidate_connection();
                anyhow::anyhow!(
                    "SillyTavern is not reachable at '{}': {}",
                    self.base_url,
                    e
                )
            })?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 403 {
                self.invalidate_connection();
            }
            anyhow::bail!(
                "Failed to get settings: HTTP {} from {}",
                resp.status(),
                url
            );
        }

        let response_body: serde_json::Value = resp.json().await.context(
            "Failed to parse settings response from SillyTavern",
        )?;

        // ST's /api/settings/get returns { "settings": "<json string>", ... }
        // The "settings" field is a JSON-encoded string that we need to parse.
        let settings = if let Some(settings_str) = response_body.get("settings").and_then(|v| v.as_str()) {
            serde_json::from_str(settings_str).context(
                "Failed to parse inner settings JSON string from SillyTavern response",
            )?
        } else if response_body.get("power_user").is_some() {
            // Fallback: if the response already has power_user at the top level,
            // it might be a newer ST version that returns parsed settings directly
            response_body
        } else {
            // Last fallback: return the response as-is and let callers handle it
            debug!("Settings response has no 'settings' string field and no 'power_user' — returning raw response");
            response_body
        };

        Ok(settings)
    }

    /// Save the raw settings JSON back to SillyTavern.
    async fn save_settings_raw(&mut self, settings: &serde_json::Value) -> Result<()> {
        let url = format!("{}/api/settings/save", self.base_url);
        debug!(url = %url, "Saving settings");

        let resp = self.post_request(&url)
            .json(settings)
            .send()
            .await
            .map_err(|e| {
                self.invalidate_connection();
                anyhow::anyhow!(
                    "SillyTavern is not reachable at '{}': {}",
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
                "Failed to save settings: HTTP {} — {}",
                status,
                body
            );
        }

        debug!("Settings saved");
        Ok(())
    }

    // ─── Character export endpoint ──────────────────────────────────────────

    /// Export a character's full JSON data from SillyTavern.
    ///
    /// Returns the raw JSON value representing the complete character export.
    pub async fn export_character(&mut self, avatar_url: &str) -> Result<serde_json::Value> {
        self.ensure_csrf().await?;

        let url = format!("{}/api/characters/export", self.base_url);
        debug!(url = %url, avatar_url = %avatar_url, "Exporting character");

        let body = serde_json::json!({ "avatar_url": avatar_url, "format": "json" });

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
                avatar_url,
                status,
                body_text
            );
        }

        let export: serde_json::Value = resp.json().await.with_context(|| {
            format!("Failed to parse export data for character '{avatar_url}' from SillyTavern")
        })?;

        debug!(avatar_url = %avatar_url, "Character exported");
        Ok(export)
    }
}
