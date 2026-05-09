//! Lorebook module — keyword-triggered context injection.
//!
//! Implements a SillyTavern-style World Info / lorebook system for the agent's
//! prompt extensions. Entries are registered with keyword lists and content.
//! When the user's message (or recent conversation) contains matching keywords,
//! the corresponding content is injected into the LLM context as document chunks.
//!
//! This allows prompt extensions (character card rules, post-history instructions,
//! world information guidelines) to be loaded on-demand rather than always consuming
//! token budget.

pub mod defaults;

use std::collections::HashSet;

use crate::context::DocumentChunk;

/// A single lorebook entry — maps keywords to injectable content.
#[derive(Debug, Clone)]
pub struct LorebookEntry {
    /// Unique identifier for this entry.
    pub id: String,
    /// Human-readable name (used as the DocumentChunk source attribution).
    pub name: String,
    /// Keywords that trigger this entry. Case-insensitive matching.
    /// Any single keyword match activates the entry.
    pub keywords: Vec<String>,
    /// The content to inject when triggered.
    pub content: String,
    /// Whether this entry is currently enabled.
    pub enabled: bool,
    /// Priority for ordering when multiple entries match (higher = injected first).
    pub priority: i32,
    /// Whether to match keywords only as whole words (true) or as substrings (false).
    pub whole_word: bool,
    /// Scan depth — how many recent messages to scan (0 = only the latest user message).
    pub scan_depth: usize,
    /// Whether this entry is "always active" (injected regardless of keyword match).
    pub constant: bool,
}

impl LorebookEntry {
    /// Create a new entry with sensible defaults.
    pub fn new(id: impl Into<String>, name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            keywords: Vec::new(),
            content: content.into(),
            enabled: true,
            priority: 0,
            whole_word: true,
            scan_depth: 2,
            constant: false,
        }
    }

    /// Builder: set keywords.
    pub fn with_keywords(mut self, keywords: Vec<impl Into<String>>) -> Self {
        self.keywords = keywords.into_iter().map(|k| k.into()).collect();
        self
    }

    /// Builder: set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Builder: set whole-word matching.
    pub fn with_whole_word(mut self, whole_word: bool) -> Self {
        self.whole_word = whole_word;
        self
    }

    /// Builder: set scan depth.
    pub fn with_scan_depth(mut self, depth: usize) -> Self {
        self.scan_depth = depth;
        self
    }

    /// Builder: mark as always-active.
    pub fn as_constant(mut self) -> Self {
        self.constant = true;
        self
    }
}

/// The lorebook — holds all entries and performs keyword scanning.
#[derive(Debug, Clone)]
pub struct Lorebook {
    entries: Vec<LorebookEntry>,
}

impl Lorebook {
    /// Create an empty lorebook.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add an entry to the lorebook.
    pub fn add_entry(&mut self, entry: LorebookEntry) {
        self.entries.push(entry);
    }

    /// Remove an entry by ID.
    pub fn remove_entry(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < before
    }

    /// Get all entries (for inspection/editing).
    pub fn entries(&self) -> &[LorebookEntry] {
        &self.entries
    }

    /// Get a mutable reference to an entry by ID.
    pub fn get_entry_mut(&mut self, id: &str) -> Option<&mut LorebookEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Scan conversation messages and return matched entries as DocumentChunks.
    ///
    /// # Arguments
    /// - `messages` — Recent conversation messages (newest last). The scanner
    ///   checks each entry's `scan_depth` to determine how far back to look.
    /// - `latest_user_message` — The current user message (always scanned).
    ///
    /// # Returns
    /// A Vec of `DocumentChunk` for all matched entries, sorted by priority (highest first).
    pub fn scan(
        &self,
        messages: &[&str],
        latest_user_message: &str,
    ) -> Vec<DocumentChunk> {
        let mut matched: Vec<&LorebookEntry> = Vec::new();
        let mut matched_ids: HashSet<&str> = HashSet::new();

        for entry in &self.entries {
            if !entry.enabled {
                continue;
            }

            // Always-active entries are always included
            if entry.constant {
                if matched_ids.insert(&entry.id) {
                    matched.push(entry);
                }
                continue;
            }

            // Build the text corpus to scan for this entry
            let mut corpus = String::new();
            corpus.push_str(latest_user_message);

            // Add recent messages up to scan_depth
            if entry.scan_depth > 0 && !messages.is_empty() {
                let start = messages.len().saturating_sub(entry.scan_depth);
                for msg in &messages[start..] {
                    corpus.push(' ');
                    corpus.push_str(msg);
                }
            }

            // Check keywords against the corpus
            if self.matches_keywords(entry, &corpus) {
                if matched_ids.insert(&entry.id) {
                    matched.push(entry);
                }
            }
        }

        // Sort by priority (highest first)
        matched.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Convert to DocumentChunks
        matched
            .into_iter()
            .map(|entry| DocumentChunk {
                source: entry.name.clone(),
                content: entry.content.clone(),
            })
            .collect()
    }

    /// Check if any of an entry's keywords match in the given text.
    fn matches_keywords(&self, entry: &LorebookEntry, text: &str) -> bool {
        if entry.keywords.is_empty() {
            return false;
        }

        let text_lower = text.to_lowercase();

        for keyword in &entry.keywords {
            let kw_lower = keyword.to_lowercase();

            if entry.whole_word {
                // Whole-word matching: keyword must be bounded by non-alphanumeric chars
                if contains_whole_word(&text_lower, &kw_lower) {
                    return true;
                }
            } else {
                // Substring matching
                if text_lower.contains(&kw_lower) {
                    return true;
                }
            }
        }

        false
    }
}

impl Default for Lorebook {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if `text` contains `word` as a whole word (bounded by non-alphanumeric or string edges).
fn contains_whole_word(text: &str, word: &str) -> bool {
    let text_bytes = text.as_bytes();
    let word_bytes = word.as_bytes();
    let word_len = word_bytes.len();
    let text_len = text_bytes.len();

    if word_len > text_len {
        return false;
    }

    // Use a simple sliding window approach
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let abs_pos = start + pos;
        let end_pos = abs_pos + word_len;

        // Check left boundary
        let left_ok = abs_pos == 0
            || !text_bytes[abs_pos - 1].is_ascii_alphanumeric();

        // Check right boundary
        let right_ok = end_pos >= text_len
            || !text_bytes[end_pos].is_ascii_alphanumeric();

        if left_ok && right_ok {
            return true;
        }

        // Move past this occurrence
        start = abs_pos + 1;
        if start >= text_len {
            break;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whole_word_matching() {
        assert!(contains_whole_word("build me a character", "character"));
        assert!(contains_whole_word("character card creation", "character"));
        assert!(contains_whole_word("the character's voice", "character"));
        assert!(!contains_whole_word("characteristics of", "character"));
        assert!(contains_whole_word("create a world", "world"));
        assert!(!contains_whole_word("worldbuilding notes", "world"));
    }

    #[test]
    fn test_substring_matching() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("test", "Test Entry", "Test content")
                .with_keywords(vec!["world"])
                .with_whole_word(false),
        );

        let chunks = lorebook.scan(&[], "worldbuilding is fun");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_case_insensitive() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("test", "Test Entry", "Test content")
                .with_keywords(vec!["Character"]),
        );

        let chunks = lorebook.scan(&[], "build me a CHARACTER card");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_no_match() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("test", "Test Entry", "Test content")
                .with_keywords(vec!["character", "persona"]),
        );

        let chunks = lorebook.scan(&[], "tell me about the weather");
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_multiple_entries_priority_order() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("low", "Low Priority", "Low content")
                .with_keywords(vec!["character"])
                .with_priority(1),
        );
        lorebook.add_entry(
            LorebookEntry::new("high", "High Priority", "High content")
                .with_keywords(vec!["character"])
                .with_priority(10),
        );

        let chunks = lorebook.scan(&[], "build a character");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].source, "High Priority");
        assert_eq!(chunks[1].source, "Low Priority");
    }

    #[test]
    fn test_constant_entry_always_included() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("always", "Always Active", "Always here")
                .as_constant(),
        );

        let chunks = lorebook.scan(&[], "completely unrelated message");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].source, "Always Active");
    }

    #[test]
    fn test_disabled_entry_skipped() {
        let mut lorebook = Lorebook::new();
        let mut entry = LorebookEntry::new("test", "Test", "Content")
            .with_keywords(vec!["character"]);
        entry.enabled = false;
        lorebook.add_entry(entry);

        let chunks = lorebook.scan(&[], "build a character");
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_scan_depth() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("deep", "Deep Scan", "Deep content")
                .with_keywords(vec!["character"])
                .with_scan_depth(3),
        );

        // Keyword is in older messages, not the latest
        let messages = vec![
            "I want to build a character",
            "something else",
            "another thing",
        ];
        let chunks = lorebook.scan(&messages, "what do you think?");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_scan_depth_zero_only_latest() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("shallow", "Shallow Scan", "Shallow content")
                .with_keywords(vec!["character"])
                .with_scan_depth(0),
        );

        // Keyword only in older messages
        let messages = vec!["I want to build a character"];
        let chunks = lorebook.scan(&messages, "what do you think?");
        assert_eq!(chunks.len(), 0);

        // Keyword in latest message
        let chunks = lorebook.scan(&messages, "build a character please");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_remove_entry() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("a", "Entry A", "Content A")
                .with_keywords(vec!["test"]),
        );
        lorebook.add_entry(
            LorebookEntry::new("b", "Entry B", "Content B")
                .with_keywords(vec!["test"]),
        );

        assert_eq!(lorebook.entries().len(), 2);
        assert!(lorebook.remove_entry("a"));
        assert_eq!(lorebook.entries().len(), 1);
        assert_eq!(lorebook.entries()[0].id, "b");
    }

    #[test]
    fn test_multiple_keywords_any_match() {
        let mut lorebook = Lorebook::new();
        lorebook.add_entry(
            LorebookEntry::new("test", "Test", "Content")
                .with_keywords(vec!["character", "persona", "NPC", "card"]),
        );

        // Only one keyword needs to match
        assert_eq!(lorebook.scan(&[], "build a persona").len(), 1);
        assert_eq!(lorebook.scan(&[], "create an NPC").len(), 1);
        assert_eq!(lorebook.scan(&[], "edit the card").len(), 1);
        assert_eq!(lorebook.scan(&[], "unrelated message").len(), 0);
    }
}
