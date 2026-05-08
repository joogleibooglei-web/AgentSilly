//! Search index module — BM25 full-text search via tantivy.
//!
//! Provides a `SearchIndex` that indexes world entries, character data, and
//! reference document chunks for fast full-text retrieval. Uses tantivy's
//! BM25 scoring for relevance ranking.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};
use tracing::{debug, info};

/// A single search result with source attribution.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// The document ID (entity ID).
    pub id: String,
    /// Source type: "world_entry", "character", "document_chunk".
    pub source_type: String,
    /// Title or label of the document.
    pub title: String,
    /// Content snippet (may be truncated).
    pub content: String,
    /// BM25 relevance score.
    pub score: f32,
}

/// BM25 full-text search index backed by tantivy.
///
/// Indexes documents with fields: id, source_type, title, content.
/// Supports querying across all fields with optional source_type filtering.
pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: std::sync::Mutex<IndexWriter>,
    schema: Schema,
    // Field handles for quick access
    field_id: Field,
    field_source_type: Field,
    field_title: Field,
    field_content: Field,
}

impl SearchIndex {
    /// Create a new search index at the given directory path.
    ///
    /// If the directory doesn't exist, it will be created.
    /// If an index already exists at the path, it will be opened.
    pub fn new(path: &Path) -> Result<Self> {
        // Build schema
        let mut schema_builder = Schema::builder();

        let field_id = schema_builder.add_text_field("id", STRING | STORED);
        let field_source_type = schema_builder.add_text_field("source_type", STRING | STORED);
        let field_title = schema_builder.add_text_field("title", TEXT | STORED);
        let field_content = schema_builder.add_text_field("content", TEXT | STORED);

        let schema = schema_builder.build();

        // Create or open the index
        std::fs::create_dir_all(path)
            .with_context(|| format!("Failed to create search index directory: {}", path.display()))?;

        let index = Index::create_in_dir(path, schema.clone())
            .or_else(|_| Index::open_in_dir(path))
            .with_context(|| format!("Failed to create/open tantivy index at: {}", path.display()))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("Failed to create index reader")?;

        let writer = index
            .writer(50_000_000) // 50MB heap
            .context("Failed to create index writer")?;

        info!(path = %path.display(), "Search index initialized");

        Ok(Self {
            index,
            reader,
            writer: std::sync::Mutex::new(writer),
            schema,
            field_id,
            field_source_type,
            field_title,
            field_content,
        })
    }

    /// Create an in-memory search index (for testing).
    pub fn new_in_memory() -> Result<Self> {
        let mut schema_builder = Schema::builder();

        let field_id = schema_builder.add_text_field("id", STRING | STORED);
        let field_source_type = schema_builder.add_text_field("source_type", STRING | STORED);
        let field_title = schema_builder.add_text_field("title", TEXT | STORED);
        let field_content = schema_builder.add_text_field("content", TEXT | STORED);

        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema.clone());

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .context("Failed to create index reader")?;

        let writer = index
            .writer(15_000_000) // 15MB heap for in-memory
            .context("Failed to create index writer")?;

        Ok(Self {
            index,
            reader,
            writer: std::sync::Mutex::new(writer),
            schema,
            field_id,
            field_source_type,
            field_title,
            field_content,
        })
    }

    /// Index a document (world entry, character data, or reference doc chunk).
    ///
    /// If a document with the same ID already exists, it will be replaced.
    pub fn index_document(
        &self,
        id: &str,
        source_type: &str,
        title: &str,
        content: &str,
    ) -> Result<()> {
        let mut writer = self.writer.lock()
            .map_err(|e| anyhow::anyhow!("Writer lock poisoned: {}", e))?;

        // Delete existing document with same ID (upsert behavior)
        let id_term = tantivy::Term::from_field_text(self.field_id, id);
        writer.delete_term(id_term);

        // Add the new document
        writer.add_document(doc!(
            self.field_id => id,
            self.field_source_type => source_type,
            self.field_title => title,
            self.field_content => content,
        ))?;

        writer.commit().context("Failed to commit index write")?;
        self.reader.reload().context("Failed to reload index reader")?;

        debug!(id = %id, source_type = %source_type, title = %title, "Document indexed");
        Ok(())
    }

    /// Remove a document from the index by ID.
    pub fn remove_document(&self, id: &str) -> Result<()> {
        let mut writer = self.writer.lock()
            .map_err(|e| anyhow::anyhow!("Writer lock poisoned: {}", e))?;

        let id_term = tantivy::Term::from_field_text(self.field_id, id);
        writer.delete_term(id_term);
        writer.commit().context("Failed to commit index delete")?;
        self.reader.reload().context("Failed to reload index reader")?;

        debug!(id = %id, "Document removed from index");
        Ok(())
    }

    /// Search the index by text query, returning top-N results.
    ///
    /// Searches across both title and content fields.
    /// Optionally filters by source_type.
    pub fn search(
        &self,
        query_text: &str,
        limit: usize,
        source_type_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();

        // Build query parser that searches across title and content
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.field_title, self.field_content],
        );

        let query = query_parser
            .parse_query(query_text)
            .with_context(|| format!("Failed to parse search query: '{}'", query_text))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .context("Search execution failed")?;

        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .context("Failed to retrieve document from index")?;

            let id = retrieved_doc
                .get_first(self.field_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let doc_source_type = retrieved_doc
                .get_first(self.field_source_type)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Apply source_type filter if specified
            if let Some(filter) = source_type_filter {
                if doc_source_type != filter {
                    continue;
                }
            }

            let title = retrieved_doc
                .get_first(self.field_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content = retrieved_doc
                .get_first(self.field_content)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Truncate content for display (max 500 chars)
            let content_snippet = if content.len() > 500 {
                format!("{}...", &content[..500])
            } else {
                content
            };

            results.push(SearchResult {
                id,
                source_type: doc_source_type,
                title,
                content: content_snippet,
                score,
            });
        }

        Ok(results)
    }

    /// Get the number of documents in the index.
    pub fn doc_count(&self) -> u64 {
        let searcher = self.reader.searcher();
        searcher.num_docs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (SearchIndex, TempDir) {
        let dir = TempDir::new().unwrap();
        let index = SearchIndex::new(dir.path()).unwrap();
        (index, dir)
    }

    #[test]
    fn test_create_index() {
        let (_index, _dir) = setup();
    }

    #[test]
    fn test_index_and_search() {
        let (index, _dir) = setup();

        index.index_document(
            "entry-1",
            "world_entry",
            "Dragon Lore",
            "Dragons are ancient creatures of immense power.",
        ).unwrap();

        index.index_document(
            "entry-2",
            "world_entry",
            "Elf History",
            "Elves are immortal beings who live in enchanted forests.",
        ).unwrap();

        // Search for "dragon"
        let results = index.search("dragon", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "entry-1");
        assert_eq!(results[0].title, "Dragon Lore");
        assert!(results[0].score > 0.0);

        // Search for "ancient"
        let results = index.search("ancient", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "entry-1");
    }

    #[test]
    fn test_search_multiple_results() {
        let (index, _dir) = setup();

        index.index_document("1", "world_entry", "Fire Dragon", "A dragon of fire.").unwrap();
        index.index_document("2", "world_entry", "Ice Dragon", "A dragon of ice.").unwrap();
        index.index_document("3", "character", "Dragon Slayer", "A warrior who hunts dragons.").unwrap();

        let results = index.search("dragon", 10, None).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_with_source_type_filter() {
        let (index, _dir) = setup();

        index.index_document("1", "world_entry", "Fire Dragon", "A dragon of fire.").unwrap();
        index.index_document("2", "character", "Dragon Slayer", "A warrior who hunts dragons.").unwrap();

        // Filter to world_entry only
        let results = index.search("dragon", 10, Some("world_entry")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_type, "world_entry");

        // Filter to character only
        let results = index.search("dragon", 10, Some("character")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_type, "character");
    }

    #[test]
    fn test_search_limit() {
        let (index, _dir) = setup();

        for i in 0..10 {
            index.index_document(
                &format!("entry-{}", i),
                "world_entry",
                &format!("Dragon Entry {}", i),
                "Dragons are everywhere in this world.",
            ).unwrap();
        }

        let results = index.search("dragon", 3, None).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_empty_index() {
        let (index, _dir) = setup();
        let results = index.search("dragon", 10, None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_remove_document() {
        let (index, _dir) = setup();

        index.index_document("1", "world_entry", "Dragon Lore", "Dragons are powerful.").unwrap();
        assert_eq!(index.doc_count(), 1);

        index.remove_document("1").unwrap();
        let results = index.search("dragon", 10, None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_upsert_document() {
        let (index, _dir) = setup();

        index.index_document("1", "world_entry", "Dragon Lore", "Original content.").unwrap();
        index.index_document("1", "world_entry", "Dragon Lore", "Updated content about dragons.").unwrap();

        // Should only have one document
        let results = index.search("dragon", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("Updated"));
    }

    #[test]
    fn test_doc_count() {
        let (index, _dir) = setup();

        assert_eq!(index.doc_count(), 0);
        index.index_document("1", "world_entry", "Entry 1", "Content 1").unwrap();
        assert_eq!(index.doc_count(), 1);
        index.index_document("2", "world_entry", "Entry 2", "Content 2").unwrap();
        assert_eq!(index.doc_count(), 2);
    }

    #[test]
    fn test_in_memory_index() {
        let index = SearchIndex::new_in_memory().unwrap();
        index.index_document("1", "world_entry", "Test", "Test content").unwrap();
        let results = index.search("test", 10, None).unwrap();
        assert_eq!(results.len(), 1);
    }
}
