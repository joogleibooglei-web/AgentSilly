//! Tool: fetch_wiki_page — fetches structured character/article data from a fandom wiki.
//!
//! Retrieves page content from a MediaWiki-compatible API, including:
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

/// Tool that fetches structured data from a fandom wiki page.
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
        "Fetch structured data from a fandom wiki page. Can retrieve the infobox (structured character/item data), specific sections by name, or list all available sections. Use after search_wiki identifies the page you want."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "page": {
                    "type": "string",
                    "description": "The page title to fetch (e.g., 'Anakin Skywalker', 'Coruscant')"
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
                    "description": "Optional override for the wiki base URL (e.g., 'https://lotr.fandom.com')"
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

        let base_url = args
            .get("wiki_url")
            .and_then(|v| v.as_str())
            .map(|s| s.trim_end_matches('/').to_string())
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
        let mut infobox = serde_json::Map::new();

        // Match lines starting with |key=value
        let field_re = Regex::new(r"^\|(\w+)\s*=\s*(.+)$").unwrap();
        let link_re = Regex::new(r"\[\[([^|\]]*\|)?([^\]]*)\]\]").unwrap();
        let template_re = Regex::new(r"\{\{[^}]*\}\}").unwrap();
        let html_re = Regex::new(r"<[^>]+>").unwrap();
        let ref_re = Regex::new(r"''[^']*''").unwrap();

        for line in wikitext.lines() {
            let line = line.trim();
            if let Some(caps) = field_re.captures(line) {
                let key = caps.get(1).unwrap().as_str().to_string();
                let raw_value = caps.get(2).unwrap().as_str().to_string();

                // Clean up the value
                let value = link_re.replace_all(&raw_value, "$2").to_string();
                let value = template_re.replace_all(&value, "").to_string();
                let value = html_re.replace_all(&value, "").to_string();
                let value = ref_re.replace_all(&value, "").to_string();
                let value = value.trim().to_string();

                // Skip empty values and image fields
                if !value.is_empty()
                    && !key.starts_with("image")
                    && !key.starts_with("option")
                    && key != "type"
                {
                    // Handle list values (lines starting with *)
                    if value.contains("\n*") || value.starts_with('*') {
                        let items: Vec<String> = value
                            .split('*')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        infobox.insert(key, serde_json::json!(items));
                    } else {
                        infobox.insert(key, Value::String(value));
                    }
                }
            }
        }

        Ok(Value::Object(infobox))
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

        // Valid: with wiki_url override
        let valid_url = serde_json::json!({
            "page": "Gandalf",
            "wiki_url": "https://lotr.fandom.com"
        });
        assert!(tool.validate_args(&valid_url).is_ok());

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
}
