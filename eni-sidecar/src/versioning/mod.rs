//! Version history module — entity snapshots for undo support.
//!
//! Provides a `VersionStore` that records entity state snapshots into the
//! `entity_versions` SQLite table. Supports snapshot, undo (pop latest),
//! listing history, and pruning old versions.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::db::Database;

/// A single version entry from the entity_versions table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    /// Auto-incremented row ID.
    pub id: i64,
    /// Entity type (e.g. "character", "world_entry").
    pub entity_type: String,
    /// Entity identifier (e.g. character name).
    pub entity_id: String,
    /// JSON snapshot of the entity state at this point in time.
    pub data: serde_json::Value,
    /// Human-readable summary of what changed.
    pub summary: String,
    /// ISO 8601 timestamp of when this version was created.
    pub created_at: String,
}

/// Stores entity version snapshots in SQLite for undo support.
///
/// Uses a shared `Database` behind a `std::sync::Mutex` since rusqlite
/// is synchronous. SQLite operations are fast enough that holding the
/// mutex briefly in an async context is acceptable.
pub struct VersionStore {
    db: Arc<Mutex<Database>>,
}

impl VersionStore {
    /// Create a new `VersionStore` with a shared database reference.
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// Snapshot the current state of an entity.
    ///
    /// Inserts a new row into `entity_versions` and then prunes to keep
    /// only the last 20 versions for this entity.
    pub fn snapshot(
        &self,
        entity_type: &str,
        entity_id: &str,
        data: &serde_json::Value,
        summary: &str,
    ) -> Result<()> {
        let data_str = serde_json::to_string(data)
            .context("Failed to serialize entity data for snapshot")?;

        let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

        db.conn().execute(
            "INSERT INTO entity_versions (entity_type, entity_id, data, summary) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![entity_type, entity_id, data_str, summary],
        ).context("Failed to insert entity version snapshot")?;

        debug!(
            entity_type = %entity_type,
            entity_id = %entity_id,
            summary = %summary,
            "Version snapshot saved"
        );

        // Prune old versions, keeping only the last 20
        self.prune_inner(&db, entity_type, entity_id, 20)?;

        Ok(())
    }

    /// Pop the most recent version for an entity and return its data.
    ///
    /// Deletes the version row after retrieving it (undo consumes the snapshot).
    /// Returns `None` if no versions exist for this entity.
    pub fn undo(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

        // Find the most recent version
        let result: Option<(i64, String)> = db
            .conn()
            .query_row(
                "SELECT id, data FROM entity_versions \
                 WHERE entity_type = ?1 AND entity_id = ?2 \
                 ORDER BY id DESC LIMIT 1",
                rusqlite::params![entity_type, entity_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .context("Failed to query latest entity version")?;

        let Some((id, data_str)) = result else {
            return Ok(None);
        };

        // Delete the version (pop)
        db.conn()
            .execute(
                "DELETE FROM entity_versions WHERE id = ?1",
                rusqlite::params![id],
            )
            .context("Failed to delete entity version after undo")?;

        let data: serde_json::Value = serde_json::from_str(&data_str)
            .context("Failed to parse stored entity version data as JSON")?;

        debug!(
            entity_type = %entity_type,
            entity_id = %entity_id,
            version_id = id,
            "Undo: popped version"
        );

        Ok(Some(data))
    }

    /// List all version entries for an entity, ordered newest first.
    pub fn list_versions(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<VersionEntry>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

        let mut stmt = db
            .conn()
            .prepare(
                "SELECT id, entity_type, entity_id, data, summary, created_at \
                 FROM entity_versions \
                 WHERE entity_type = ?1 AND entity_id = ?2 \
                 ORDER BY id DESC",
            )
            .context("Failed to prepare list_versions query")?;

        let entries = stmt
            .query_map(rusqlite::params![entity_type, entity_id], |row| {
                let data_str: String = row.get(3)?;
                Ok(VersionEntry {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_id: row.get(2)?,
                    data: serde_json::from_str(&data_str).unwrap_or(serde_json::Value::Null),
                    summary: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .context("Failed to execute list_versions query")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to read version entries from database")?;

        Ok(entries)
    }

    /// Keep only the last `keep` versions for an entity, deleting older ones.
    pub fn prune(
        &self,
        entity_type: &str,
        entity_id: &str,
        keep: usize,
    ) -> Result<()> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        self.prune_inner(&db, entity_type, entity_id, keep)
    }

    /// Internal prune implementation that operates on an already-locked database.
    fn prune_inner(
        &self,
        db: &Database,
        entity_type: &str,
        entity_id: &str,
        keep: usize,
    ) -> Result<()> {
        // Delete all but the most recent `keep` versions
        db.conn()
            .execute(
                "DELETE FROM entity_versions \
                 WHERE entity_type = ?1 AND entity_id = ?2 \
                 AND id NOT IN ( \
                     SELECT id FROM entity_versions \
                     WHERE entity_type = ?1 AND entity_id = ?2 \
                     ORDER BY id DESC LIMIT ?3 \
                 )",
                rusqlite::params![entity_type, entity_id, keep as i64],
            )
            .context("Failed to prune old entity versions")?;

        Ok(())
    }
}

/// Extension trait to add `.optional()` to rusqlite results.
trait OptionalExt<T> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_store() -> VersionStore {
        let db = Database::open(":memory:").unwrap();
        let db = Arc::new(Mutex::new(db));
        VersionStore::new(db)
    }

    #[test]
    fn test_snapshot_and_list() {
        let store = setup_store();
        let data = serde_json::json!({"name": "Kael", "description": "A warrior"});

        store.snapshot("character", "kael", &data, "Initial state").unwrap();

        let versions = store.list_versions("character", "kael").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].entity_type, "character");
        assert_eq!(versions[0].entity_id, "kael");
        assert_eq!(versions[0].data["name"], "Kael");
        assert_eq!(versions[0].summary, "Initial state");
    }

    #[test]
    fn test_undo_pops_latest() {
        let store = setup_store();

        let data1 = serde_json::json!({"name": "Kael", "description": "Version 1"});
        let data2 = serde_json::json!({"name": "Kael", "description": "Version 2"});

        store.snapshot("character", "kael", &data1, "v1").unwrap();
        store.snapshot("character", "kael", &data2, "v2").unwrap();

        // Undo should return the latest (v2)
        let undone = store.undo("character", "kael").unwrap();
        assert!(undone.is_some());
        assert_eq!(undone.unwrap()["description"], "Version 2");

        // After undo, only v1 remains
        let versions = store.list_versions("character", "kael").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].data["description"], "Version 1");
    }

    #[test]
    fn test_undo_empty_returns_none() {
        let store = setup_store();
        let result = store.undo("character", "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_prune_keeps_only_n() {
        let store = setup_store();

        // Insert 5 versions
        for i in 0..5 {
            let data = serde_json::json!({"version": i});
            store.snapshot("character", "kael", &data, &format!("v{}", i)).unwrap();
        }

        // Prune to keep only 2
        store.prune("character", "kael", 2).unwrap();

        let versions = store.list_versions("character", "kael").unwrap();
        assert_eq!(versions.len(), 2);
        // Should keep the most recent (v4 and v3)
        assert_eq!(versions[0].data["version"], 4);
        assert_eq!(versions[1].data["version"], 3);
    }

    #[test]
    fn test_snapshot_auto_prunes_to_20() {
        let store = setup_store();

        // Insert 25 versions
        for i in 0..25 {
            let data = serde_json::json!({"version": i});
            store.snapshot("character", "kael", &data, &format!("v{}", i)).unwrap();
        }

        let versions = store.list_versions("character", "kael").unwrap();
        assert_eq!(versions.len(), 20);
        // Most recent should be v24
        assert_eq!(versions[0].data["version"], 24);
    }

    #[test]
    fn test_different_entities_are_independent() {
        let store = setup_store();

        let data_a = serde_json::json!({"name": "A"});
        let data_b = serde_json::json!({"name": "B"});

        store.snapshot("character", "a", &data_a, "char a").unwrap();
        store.snapshot("character", "b", &data_b, "char b").unwrap();

        let versions_a = store.list_versions("character", "a").unwrap();
        let versions_b = store.list_versions("character", "b").unwrap();

        assert_eq!(versions_a.len(), 1);
        assert_eq!(versions_b.len(), 1);
        assert_eq!(versions_a[0].data["name"], "A");
        assert_eq!(versions_b[0].data["name"], "B");
    }
}
