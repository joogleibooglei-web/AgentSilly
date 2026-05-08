//! Tool: search_local — BM25 full-text search via tantivy.
//!
//! Provides local full-text search across world entries, character data,
//! and reference document chunks using the tantivy search engine.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use crate::search::SearchIndex;

/// Tool that performs BM25 full-text search across local data
/// (world entries, character data, reference document chunks).
pub struct SearchLocalTool {
    search_index: Arc<SearchIndex>,
}

impl SearchLocalTool {
    /// Create a new `SearchLocalTool` with a shared search index.
    pub fn new(search_index: Arc<SearchIndex>) -> Self {
        Self { search_index }
    }
}

#[async_trait]
impl Tool for SearchLocalTool {
    fn name(&self) -> &str {
        "search_local"
    }

    fn description(&self) -> &str {
        "Search local world entries, character data, and reference documents using full-text search. Returns the most relevant matches with source attribution."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query text"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default 10, max 50)",
                    "default": 10
                },
                "source_type": {
                    "type": "string",
                    "description": "Optional filter by source type: 'world_entry', 'character', 'document_chunk'",
                    "enum": ["world_entry", "character", "document_chunk"]
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
            .unwrap_or(10)
            .min(50) as usize;

        let source_type = args
            .get("source_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        debug!(query = %query, limit = limit, source_type = ?source_type, "Searching local index");

        let results = self.search_index.search(query, limit, source_type.as_deref())?;

        debug!(count = results.len(), "Local search returned results");

        Ok(serde_json::json!({
            "query": query,
            "results": results,
            "total": results.len()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchIndex;
    use tempfile::TempDir;

    fn setup_index() -> (Arc<SearchIndex>, TempDir) {
        let dir = TempDir::new().unwrap();
        let index = SearchIndex::new(dir.path()).unwrap();
        (Arc::new(index), dir)
    }

    #[test]
    fn test_schema_validation() {
        let (index, _dir) = setup_index();
        let tool = SearchLocalTool::new(index);

        // Valid: has query
        let valid = serde_json::json!({"query": "dragon lore"});
        assert!(tool.validate_args(&valid).is_ok());

        // Valid: with limit and source_type
        let valid_full = serde_json::json!({
            "query": "cyberpunk",
            "limit": 5,
            "source_type": "world_entry"
        });
        assert!(tool.validate_args(&valid_full).is_ok());

        // Invalid: missing query
        let invalid = serde_json::json!({"limit": 5});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[tokio::test]
    async fn test_search_empty_index() {
        let (index, _dir) = setup_index();
        let tool = SearchLocalTool::new(index);

        let args = serde_json::json!({"query": "dragon"});
        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total"], 0);
        assert_eq!(result["results"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_search_with_indexed_content() {
        let (index, _dir) = setup_index();

        // Index some documents
        index.index_document(
            "entry-1",
            "world_entry",
            "Dragon Lore",
            "Dragons are ancient creatures of immense power that dwell in mountains.",
        ).unwrap();

        index.index_document(
            "entry-2",
            "world_entry",
            "Elf History",
            "Elves are immortal beings who live in forests.",
        ).unwrap();

        let tool = SearchLocalTool::new(index);

        // Search for "dragon"
        let args = serde_json::json!({"query": "dragon"});
        let result = tool.execute(args).await.unwrap();
        assert!(result["total"].as_i64().unwrap() >= 1);

        let results = result["results"].as_array().unwrap();
        assert!(results.iter().any(|r| r["title"].as_str().unwrap().contains("Dragon")));
    }
}
