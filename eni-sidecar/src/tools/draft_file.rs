//! Draft file I/O utilities for ephemeral draft storage.
//!
//! Provides constants for draft file paths and async functions for
//! creating, reading, writing, and deleting draft files under `/tmp/eni-sidecar/`.

use std::path::Path;

use anyhow::Result;
use tokio::fs;

/// Base directory for all draft files.
pub const DRAFT_DIR: &str = "/tmp/eni-sidecar";

/// Fixed path for the world info draft.
pub const WORLD_DRAFT_PATH: &str = "/tmp/eni-sidecar/world_draft.txt";

/// Fixed path for the post-history draft.
pub const POST_HISTORY_DRAFT_PATH: &str = "/tmp/eni-sidecar/post_history_draft.txt";

/// Ensure the draft directory exists, creating it if necessary.
pub async fn ensure_draft_dir() -> Result<()> {
    fs::create_dir_all(DRAFT_DIR).await?;
    Ok(())
}

/// Read a draft file. Returns None if the file does not exist.
pub async fn read_draft(path: &str) -> Result<Option<String>> {
    match fs::read_to_string(path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Write content to a draft file. Creates the directory if needed.
/// Returns true if a previous draft existed (was overwritten).
pub async fn write_draft(path: &str, content: &str) -> Result<bool> {
    ensure_draft_dir().await?;
    let existed = Path::new(path).exists();
    fs::write(path, content).await?;
    Ok(existed)
}

/// Delete a draft file. No-op if the file doesn't exist.
pub async fn delete_draft(path: &str) -> Result<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Replace the first occurrence of `old_text` in `content` with `new_text`.
/// Returns the modified string, or None if `old_text` was not found.
pub fn str_replace_first(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    if let Some(pos) = content.find(old_text) {
        let mut result = String::with_capacity(content.len() - old_text.len() + new_text.len());
        result.push_str(&content[..pos]);
        result.push_str(new_text);
        result.push_str(&content[pos + old_text.len()..]);
        Some(result)
    } else {
        None
    }
}
