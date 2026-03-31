use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::{Result, RoamError};

/// Page data to write to ChronDB in batch.
#[derive(Clone)]
pub struct PageEntry {
    pub uid: String,
    pub title: String,
    pub file_path: String,
    pub block_count: usize,
}

pub struct SyncStore {
    db_dir: PathBuf,
    /// Cached page UIDs loaded once at startup.
    known_pages: HashSet<String>,
    /// Last sync timestamp (epoch millis), loaded from ChronDB.
    last_sync_ms: i64,
    /// Timestamp to save on next flush (set before sync starts).
    next_sync_ms: i64,
    /// Pending writes — flushed to ChronDB in one batch at the end.
    pending: Vec<PageEntry>,
}

impl SyncStore {
    pub fn open(db_dir: &Path) -> Result<Self> {
        let db_dir = db_dir.to_path_buf();
        let (known_pages, last_sync_ms) = Self::load_state(&db_dir);

        Ok(Self {
            db_dir,
            known_pages,
            last_sync_ms,
            next_sync_ms: 0,
            pending: Vec::new(),
        })
    }

    fn data_path(&self) -> PathBuf {
        self.db_dir.join("data")
    }

    fn index_path(&self) -> PathBuf {
        self.db_dir.join("index")
    }

    fn load_state(db_dir: &Path) -> (HashSet<String>, i64) {
        let data_path = db_dir.join("data");
        let index_path = db_dir.join("index");

        if !data_path.exists() {
            return (HashSet::new(), 0);
        }

        let db = match chrondb::ChronDB::open(
            data_path.to_str().unwrap_or(""),
            index_path.to_str().unwrap_or(""),
        ) {
            Ok(db) => db,
            Err(_) => {
                // Index might be corrupted (e.g. from Ctrl+C). Reset and retry.
                eprintln!("ChronDB: index corrupted, rebuilding...");
                let _ = std::fs::remove_dir_all(&index_path);
                let _ = std::fs::create_dir_all(&index_path);
                match chrondb::ChronDB::open(
                    data_path.to_str().unwrap_or(""),
                    index_path.to_str().unwrap_or(""),
                ) {
                    Ok(db) => db,
                    Err(_) => return (HashSet::new(), 0),
                }
            }
        };

        let mut pages = HashSet::new();
        if let Ok(list) = db.list_by_prefix("page:", None) {
            if let Some(arr) = list.as_array() {
                for item in arr {
                    if let Some(key) = item.get("id").and_then(|v| v.as_str()) {
                        if let Some(uid) = key.strip_prefix("page:") {
                            pages.insert(uid.to_string());
                        }
                    }
                }
            } else if let Some(obj) = list.as_object() {
                for key in obj.keys() {
                    if let Some(uid) = key.strip_prefix("page:") {
                        pages.insert(uid.to_string());
                    }
                }
            }
        }

        // Load last sync timestamp
        let last_sync_ms = db
            .get("meta:last_sync_ms", None)
            .ok()
            .and_then(|v| v.get("ts")?.as_i64())
            .unwrap_or(0);

        (pages, last_sync_ms)
    }

    fn open_db(&self) -> Result<chrondb::ChronDB> {
        let data_path = self.data_path();
        let index_path = self.index_path();

        std::fs::create_dir_all(&data_path)
            .map_err(|e| RoamError::Generic(format!("Failed to create ChronDB data dir: {}", e)))?;
        std::fs::create_dir_all(&index_path).map_err(|e| {
            RoamError::Generic(format!("Failed to create ChronDB index dir: {}", e))
        })?;

        chrondb::ChronDB::open(
            data_path.to_str().unwrap_or(""),
            index_path.to_str().unwrap_or(""),
        )
        .map_err(|e| RoamError::Generic(format!("Failed to open ChronDB: {}", e)))
    }

    /// Fast in-memory check — no ChronDB round-trip.
    pub fn has_page(&self, uid: &str) -> bool {
        self.known_pages.contains(uid)
    }

    /// Queue a page for batch write. Updates in-memory cache immediately.
    pub fn put_page(&mut self, uid: &str, title: &str, file_path: &str, block_count: usize) {
        self.known_pages.insert(uid.to_string());
        self.pending.push(PageEntry {
            uid: uid.to_string(),
            title: title.to_string(),
            file_path: file_path.to_string(),
            block_count,
        });
    }

    /// Flush pending pages to ChronDB + always save sync timestamp.
    pub fn flush(&mut self) -> Result<()> {
        let db = self.open_db()?;

        if !self.pending.is_empty() {
            eprintln!("Writing {} pages to ChronDB...", self.pending.len());
            for entry in &self.pending {
                let key = format!("page:{}", entry.uid);
                let doc = json!({
                    "title": entry.title,
                    "uid": entry.uid,
                    "file_path": entry.file_path,
                    "block_count": entry.block_count,
                });
                if let Err(e) = db.put(&key, &doc, None) {
                    eprintln!("ChronDB: failed to write page {}: {}", entry.uid, e);
                    break;
                }
            }
        }

        // Save sync timestamp (must be JSON object, not scalar)
        if self.next_sync_ms > 0 {
            let _ = db.put("meta:last_sync_ms", &json!({"ts": self.next_sync_ms}), None);
            self.last_sync_ms = self.next_sync_ms;
        }

        drop(db);

        let count = self.pending.len();
        self.pending.clear();
        if count > 0 {
            eprintln!("ChronDB: {} pages written", count);
        }

        Ok(())
    }

    /// Get version history for a page.
    #[allow(dead_code)]
    pub fn page_history(&self, uid: &str) -> Result<Value> {
        let db = self.open_db()?;
        let key = format!("page:{}", uid);
        db.history(&key, None)
            .map_err(|e| RoamError::Generic(format!("ChronDB history error: {}", e)))
    }

    pub fn known_page_count(&self) -> usize {
        self.known_pages.len()
    }

    pub fn last_sync_ms(&self) -> i64 {
        self.last_sync_ms
    }

    /// Set the timestamp to save on next flush (call before sync starts).
    pub fn set_next_sync_ms(&mut self, ms: i64) {
        self.next_sync_ms = ms;
    }

    // --- Git remote operations on ChronDB's data dir ---
}
