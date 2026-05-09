//! Tool: fetch_wiki_page — fetches structured character/article data from any MediaWiki wiki.
//!
//! Retrieves page content from a MediaWiki-compatible API (Fandom, wiki.gg, or custom),
//! including:
//! - Infobox data (parsed from wikitext templates)
//! - Specific sections by name (e.g., "Personality and traits", "Powers and abilities")
//! - Section listing for discovery
//!
//! This complements `search_wiki` by providing deep page content after a search
//! identifies the relevant article.

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use tracing::{debug, warn};

use super::dispatcher::{validate_against_schema, Tool};

/// Well-known wiki registries that can be referenced by short name.
const KNOWN_WIKIS: &[(&str, &str)] = &[
    ("starwars", "https://starwars.fandom.com"),
    ("lotr", "https://lotr.fandom.com"),
    ("cyberpunk", "https://cyberpunk.fandom.com"),
    ("rejuvenation", "https://rejuvenation.wiki.gg"),
    ("pokemon", "https://pokemon.wiki.gg"),
    ("terraria", "https://terraria.wiki.gg"),
    ("minecraft", "https://minecraft.wiki.gg"),
    ("zelda", "https://zelda.wiki.gg"),
    ("hollowknight", "https://hollowknight.wiki.gg"),
    ("genshin", "https://genshin-impact.fandom.com"),
    ("dnd", "https://forgottenrealms.fandom.com"),
    ("wookieepedia", "https://starwars.fandom.com"),
    ("memory-alpha", "https://memory-alpha.fandom.com"),
];

/// Resolve a wiki identifier to a base URL.
/// Accepts either a known short name or a full URL.
fn resolve_wiki_url(input: &str) -> String {
    if let Some((_name, url)) = KNOWN_WIKIS.iter().find(|(name, _)| {
        name.eq_ignore_ascii_case(input)
    }) {
        return url.to_string();
    }
    input.trim_end_matches('/').to_string()
}

/// Tool that fetches structured data from any MediaWiki-compatible wiki page.
pub struct FetchWikiPageTool {
    http: reqwest::Client,
    /// Default wiki base URL.
    wiki_base_url: String,
}

impl FetchWikiPageTool {
    /// Create a new `FetchWikiPageTool`.
    ///
    /// If `wiki_base_url` is `None`, defaults to the Star Wars fandom wiki.
    pub fn new(wiki_base_url: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to build HTTP client for FetchWikiPageTool");

        let base_url = wiki_base_url
            .unwrap_or_else(|| "https://starwars.fandom.com".to_string());

        Self {
            http,
            wiki_base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl Tool for FetchWikiPageTool {
    fn name(&self) -> &str {
        "fetch_wiki_page"
    }

    fn description(&self) -> &str {
        "Fetch structured data from any MediaWiki-compatible wiki page (Fandom, wiki.gg, or custom). Can retrieve the infobox (structured character/item data), specific sections by name, or list all available sections. Supports known wiki short names (e.g., 'rejuvenation', 'starwars', 'terraria') or full URLs. Use after search_wiki identifies the page you want."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "page": {
                    "type": "string",
                    "description": "The page title to fetch (e.g., 'Anakin Skywalker', 'Nim', 'Coruscant')"
                },
                "sections": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Section names to fetch (e.g., ['Personality and traits', 'Powers and abilities']). If omitted, returns the infobox and section list."
                },
                "include_infobox": {
                    "type": "boolean",
                    "description": "Whether to parse and include the infobox data (default: true)",
                    "default": true
                },
                "wiki_url": {
                    "type": "string",
                    "description": "Wiki to fetch from. Can be a known short name (e.g., 'rejuvenation', 'starwars', 'terraria', 'minecraft', 'zelda', 'lotr', 'dnd') or a full URL (e.g., 'https://rejuvenation.wiki.gg', 'https://lotr.fandom.com'). If omitted, uses the default wiki."
                }
            },
            "required": ["page"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let page = args["page"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: page"))?;

        let sections_requested: Vec<String> = args
            .get("sections")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let include_infobox = args
            .get("include_infobox")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Resolve wiki URL — supports short names and full URLs
        let base_url = args
            .get("wiki_url")
            .and_then(|v| v.as_str())
            .map(resolve_wiki_url)
            .unwrap_or_else(|| self.wiki_base_url.clone());

        let api_url = format!("{}/api.php", base_url);
        let page_encoded = page.replace(' ', "_");

        debug!(page = %page, wiki = %base_url, "Fetching wiki page");

        let mut result = serde_json::json!({
            "page": page,
            "url": format!("{}/wiki/{}", base_url, page_encoded),
            "source": base_url
        });

        // Fetch section list
        let section_list = self.fetch_sections(&api_url, &page_encoded).await?;
        result["available_sections"] = serde_json::json!(
            section_list.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        // Fetch infobox from section 0 wikitext
        if include_infobox {
            match self.fetch_infobox(&api_url, &page_encoded).await {
                Ok(infobox) => {
                    result["infobox"] = infobox;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse infobox");
                    result["infobox"] = Value::Null;
                }
            }
        }

        // Fetch requested sections
        if !sections_requested.is_empty() {
            let mut fetched_sections = serde_json::Map::new();

            for section_name in &sections_requested {
                // Find the section index by name (case-insensitive)
                let section_info = section_list.iter().find(|s| {
                    s.name.to_lowercase() == section_name.to_lowercase()
                });

                if let Some(info) = section_info {
                    match self.fetch_section_text(&api_url, &page_encoded, &info.index).await {
                        Ok(text) => {
                            fetched_sections.insert(section_name.clone(), Value::String(text));
                        }
                        Err(e) => {
                            warn!(section = %section_name, error = %e, "Failed to fetch section");
                            fetched_sections.insert(
                                section_name.clone(),
                                Value::String(format!("Error: {}", e)),
                            );
                        }
                    }
                } else {
                    fetched_sections.insert(
                        section_name.clone(),
                        Value::String("Section not found".to_string()),
                    );
                }
            }

            result["sections"] = Value::Object(fetched_sections);
        }

        Ok(result)
    }
}

/// Metadata about a page section.
struct SectionInfo {
    name: String,
    index: String,
}

impl FetchWikiPageTool {
    /// Fetch the list of sections for a page.
    async fn fetch_sections(&self, api_url: &str, page: &str) -> Result<Vec<SectionInfo>> {
        let resp = self
            .http
            .get(api_url)
            .query(&[
                ("action", "parse"),
                ("page", page),
                ("prop", "sections"),
                ("format", "json"),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Wiki API returned HTTP {}", resp.status());
        }

        let body: Value = resp.json().await?;

        // Check for MediaWiki error responses (e.g., page not found)
        if let Some(error) = body.get("error") {
            let info = error["info"].as_str().unwrap_or("Unknown error");
            anyhow::bail!("Wiki API error: {}", info);
        }

        let sections = body
            .pointer("/parse/sections")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let name = item["line"].as_str()?.to_string();
                        let index = item["index"].as_str()?.to_string();
                        Some(SectionInfo { name, index })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(sections)
    }

    /// Fetch and parse the infobox from section 0 wikitext.
    ///
    /// Handles multiple infobox template styles:
    /// - Fandom-style: `{{Character\n|key = value\n}}`
    /// - wiki.gg-style: `{{Infobox character\n|key = value\n}}` or `{{Infobox\n|key = value\n}}`
    /// - Generic: any template in section 0 with `|key = value` fields
    async fn fetch_infobox(&self, api_url: &str, page: &str) -> Result<Value> {
        let resp = self
            .http
            .get(api_url)
            .query(&[
                ("action", "parse"),
                ("page", page),
                ("prop", "wikitext"),
                ("section", "0"),
                ("format", "json"),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Wiki API returned HTTP {}", resp.status());
        }

        let body: Value = resp.json().await?;
        let wikitext = body
            .pointer("/parse/wikitext/*")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Parse infobox fields from wikitext
        let infobox = Self::parse_infobox_fields(wikitext);

        Ok(Value::Object(infobox))
    }

    /// Parse infobox key-value fields from wikitext.
    ///
    /// This handles both Fandom and wiki.gg template styles by looking for
    /// `|key = value` patterns within template blocks.
    fn parse_infobox_fields(wikitext: &str) -> serde_json::Map<String, Value> {
        let mut infobox = serde_json::Map::new();

        // Match lines starting with |key = value (with flexible whitespace)
        let field_re = Regex::new(r"^\|([A-Za-z_][\w\s]*?)\s*=\s*(.+)$").unwrap();
        let link_re = Regex::new(r"\[\[([^|\]]*\|)?([^\]]*)\]\]").unwrap();
        let template_re = Regex::new(r"\{\{[^}]*\}\}").unwrap();
        let html_re = Regex::new(r"<[^>]+>").unwrap();
        let ref_re = Regex::new(r"''[^']*''").unwrap();
        let file_re = Regex::new(r"\[\[(?:File|Image):[^\]]*\]\]").unwrap();

        for line in wikitext.lines() {
            let line = line.trim();
            if let Some(caps) = field_re.captures(line) {
                let key = caps.get(1).unwrap().as_str().trim().to_string();
                let raw_value = caps.get(2).unwrap().as_str().to_string();

                // Skip image/file fields
                if key.to_lowercase().starts_with("image")
                    || key.to_lowercase().starts_with("icon")
                    || key.to_lowercase().starts_with("sprite")
                    || key.to_lowercase() == "type"
                    || key.to_lowercase().starts_with("option")
                {
                    continue;
                }

                // Clean up the value
                let value = file_re.replace_all(&raw_value, "").to_string();
                let value = link_re.replace_all(&value, "$2").to_string();
                let value = template_re.replace_all(&value, "").to_string();
                let value = html_re.replace_all(&value, "").to_string();
                let value = ref_re.replace_all(&value, "").to_string();
                let value = value.trim().to_string();

                // Skip empty values
                if value.is_empty() {
                    continue;
                }

                // Handle list values (comma-separated or newline-separated with *)
                if value.contains("\n*") || value.starts_with('*') {
                    let items: Vec<String> = value
                        .split('*')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    infobox.insert(key, serde_json::json!(items));
                } else if value.contains("<br") || value.contains(",") {
                    // Handle <br>-separated or comma-separated lists
                    let separator = if value.contains("<br") { "<br" } else { "," };
                    let items: Vec<String> = value
                        .split(separator)
                        .map(|s| {
                            // Clean any remaining HTML fragments
                            let cleaned = Regex::new(r"[/>]")
                                .unwrap()
                                .replace_all(s.trim(), "")
                                .trim()
                                .to_string();
                            cleaned
                        })
                        .filter(|s| !s.is_empty())
                        .collect();

                    if items.len() > 1 {
                        infobox.insert(key, serde_json::json!(items));
                    } else {
                        infobox.insert(key, Value::String(value));
                    }
                } else {
                    infobox.insert(key, Value::String(value));
                }
            }
        }

        infobox
    }

    /// Fetch a specific section's text content (HTML stripped to plaintext).
    async fn fetch_section_text(
        &self,
        api_url: &str,
        page: &str,
        section_index: &str,
    ) -> Result<String> {
        let resp = self
            .http
            .get(api_url)
            .query(&[
                ("action", "parse"),
                ("page", page),
                ("prop", "text"),
                ("section", section_index),
                ("format", "json"),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Wiki API returned HTTP {}", resp.status());
        }

        let body: Value = resp.json().await?;
        let html = body
            .pointer("/parse/text/*")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Strip HTML tags
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let text = tag_re.replace_all(html, "");

        // Collapse whitespace
        let ws_re = Regex::new(r"\s+").unwrap();
        let text = ws_re.replace_all(&text, " ");

        // Remove citation brackets like [1], [2]
        let cite_re = Regex::new(r"\[\d+\]").unwrap();
        let text = cite_re.replace_all(&text, "");

        // Decode common HTML entities
        let text = text
            .replace("&#8213;", "—")
            .replace("&#8212;", "—")
            .replace("&#8211;", "–")
            .replace("&#8230;", "…")
            .replace("&#160;", " ")
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#91;", "[")
            .replace("&#93;", "]");

        // Trim and limit length to avoid overwhelming the LLM context
        let text = text.trim().to_string();
        if text.len() > 6000 {
            Ok(format!("{}...\n[Truncated — full article available at wiki URL]", &text[..6000]))
        } else {
            Ok(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_validation() {
        let tool = FetchWikiPageTool::new(None);

        // Valid: just page
        let valid = serde_json::json!({"page": "Anakin Skywalker"});
        assert!(tool.validate_args(&valid).is_ok());

        // Valid: with sections
        let valid_sections = serde_json::json!({
            "page": "Anakin Skywalker",
            "sections": ["Personality and traits", "Powers and abilities"]
        });
        assert!(tool.validate_args(&valid_sections).is_ok());

        // Valid: with wiki_url override (full URL)
        let valid_url = serde_json::json!({
            "page": "Gandalf",
            "wiki_url": "https://lotr.fandom.com"
        });
        assert!(tool.validate_args(&valid_url).is_ok());

        // Valid: with wiki_url override (short name)
        let valid_short = serde_json::json!({
            "page": "Nim",
            "wiki_url": "rejuvenation"
        });
        assert!(tool.validate_args(&valid_short).is_ok());

        // Invalid: missing page
        let invalid = serde_json::json!({"sections": ["Biography"]});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_default_wiki_url() {
        let tool = FetchWikiPageTool::new(None);
        assert_eq!(tool.wiki_base_url, "https://starwars.fandom.com");
    }

    #[test]
    fn test_custom_wiki_url() {
        let tool = FetchWikiPageTool::new(Some("https://lotr.fandom.com/".to_string()));
        assert_eq!(tool.wiki_base_url, "https://lotr.fandom.com");
    }

    #[test]
    fn test_resolve_wiki_url_short_name() {
        assert_eq!(resolve_wiki_url("rejuvenation"), "https://rejuvenation.wiki.gg");
        assert_eq!(resolve_wiki_url("starwars"), "https://starwars.fandom.com");
    }

    #[test]
    fn test_resolve_wiki_url_full_url() {
        assert_eq!(
            resolve_wiki_url("https://custom.wiki.gg/"),
            "https://custom.wiki.gg"
        );
    }

    #[test]
    fn test_parse_infobox_fandom_style() {
        let wikitext = r#"{{Character
|name = Anakin Skywalker
|homeworld = [[Tatooine]]
|species = [[Human]]
|gender = Male
|height = 1.88 meters
|image = anakin.png
}}"#;

        let result = FetchWikiPageTool::parse_infobox_fields(wikitext);
        assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("Anakin Skywalker"));
        assert_eq!(result.get("homeworld").and_then(|v| v.as_str()), Some("Tatooine"));
        assert_eq!(result.get("species").and_then(|v| v.as_str()), Some("Human"));
        assert_eq!(result.get("gender").and_then(|v| v.as_str()), Some("Male"));
        // image should be skipped
        assert!(result.get("image").is_none());
    }

    #[test]
    fn test_parse_infobox_wikigg_style() {
        let wikitext = r#"{{Infobox character
|name = Nim
|class = Fire
|gender = Female
|age = 17
|hair color = Red
|eye color = Amber
|icon = nim_icon.png
}}"#;

        let result = FetchWikiPageTool::parse_infobox_fields(wikitext);
        assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("Nim"));
        assert_eq!(result.get("class").and_then(|v| v.as_str()), Some("Fire"));
        assert_eq!(result.get("gender").and_then(|v| v.as_str()), Some("Female"));
        assert_eq!(result.get("hair color").and_then(|v| v.as_str()), Some("Red"));
        // icon should be skipped
        assert!(result.get("icon").is_none());
    }
}
