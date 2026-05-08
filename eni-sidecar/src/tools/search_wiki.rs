//! Tool: search_wiki — queries a fandom wiki and returns structured results.
//!
//! Performs an HTTP GET to a configurable wiki search API (defaults to fandom wiki),
//! parses the results, and returns structured summaries with title, snippet, and URL.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};

/// Tool that searches a fandom wiki (or configurable wiki URL) and returns
/// structured results with title, snippet, and URL.
pub struct SearchWikiTool {
    http: reqwest::Client,
    /// Base URL for the wiki API. Defaults to a fandom wiki search endpoint.
    /// Expected format: "https://<wiki>.fandom.com" (the /api.php path is appended).
    wiki_base_url: String,
}

impl SearchWikiTool {
    /// Create a new `SearchWikiTool` with a configurable wiki base URL.
    ///
    /// If `wiki_base_url` is `None`, defaults to a generic fandom wiki URL.
    /// The user can configure this to point at any MediaWiki-compatible API.
    pub fn new(wiki_base_url: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to build HTTP client for SearchWikiTool");

        let base_url = wiki_base_url
            .unwrap_or_else(|| "https://cyberpunk.fandom.com".to_string());

        Self {
            http,
            wiki_base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl Tool for SearchWikiTool {
    fn name(&self) -> &str {
        "search_wiki"
    }

    fn description(&self) -> &str {
        "Search a fandom wiki for information. Returns titles, snippets, and URLs for matching articles."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query to look up on the wiki"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default 5, max 20)",
                    "default": 5
                },
                "wiki_url": {
                    "type": "string",
                    "description": "Optional override for the wiki base URL (e.g., 'https://lotr.fandom.com')"
                }
            },
            "required": ["query"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(5)
            .min(20) as usize;

        // Allow per-call wiki URL override
        let base_url = args
            .get("wiki_url")
            .and_then(|v| v.as_str())
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| self.wiki_base_url.clone());

        debug!(query = %query, limit = limit, wiki = %base_url, "Searching wiki");

        // Use MediaWiki API opensearch endpoint
        let api_url = format!("{}/api.php", base_url);

        let resp = self
            .http
            .get(&api_url)
            .query(&[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", query),
                ("srlimit", &limit.to_string()),
                ("format", "json"),
                ("srprop", "snippet|titlesnippet"),
            ])
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Wiki search request failed: {}", e))?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Wiki API returned HTTP {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse wiki response: {}", e))?;

        // Parse MediaWiki search results
        let results = body
            .pointer("/query/search")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .take(limit)
                    .map(|item| {
                        let title = item["title"].as_str().unwrap_or("Unknown");
                        let snippet = item["snippet"]
                            .as_str()
                            .unwrap_or("")
                            // Strip HTML tags from MediaWiki snippets
                            .replace("<span class=\"searchmatch\">", "")
                            .replace("</span>", "")
                            .replace("&quot;", "\"")
                            .replace("&amp;", "&")
                            .replace("&lt;", "<")
                            .replace("&gt;", ">");

                        // Construct the article URL
                        let encoded_title = title.replace(' ', "_");
                        let url = format!("{}/wiki/{}", base_url, encoded_title);

                        serde_json::json!({
                            "title": title,
                            "snippet": snippet,
                            "url": url
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        debug!(count = results.len(), "Wiki search returned results");

        Ok(serde_json::json!({
            "query": query,
            "results": results,
            "source": base_url
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_validation() {
        let tool = SearchWikiTool::new(None);

        // Valid: has query
        let valid = serde_json::json!({"query": "megacorporation"});
        assert!(tool.validate_args(&valid).is_ok());

        // Valid: with limit
        let valid_limit = serde_json::json!({"query": "dragon", "limit": 10});
        assert!(tool.validate_args(&valid_limit).is_ok());

        // Valid: with wiki_url override
        let valid_url = serde_json::json!({
            "query": "elf",
            "wiki_url": "https://lotr.fandom.com"
        });
        assert!(tool.validate_args(&valid_url).is_ok());

        // Invalid: missing query
        let invalid = serde_json::json!({"limit": 5});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_default_wiki_url() {
        let tool = SearchWikiTool::new(None);
        assert_eq!(tool.wiki_base_url, "https://cyberpunk.fandom.com");
    }

    #[test]
    fn test_custom_wiki_url() {
        let tool = SearchWikiTool::new(Some("https://lotr.fandom.com/".to_string()));
        assert_eq!(tool.wiki_base_url, "https://lotr.fandom.com");
    }
}
