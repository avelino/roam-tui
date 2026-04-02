use std::collections::HashSet;
use std::path::Path;

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
    db: chrondb::ChronDB,
    /// Cached page UIDs loaded once at startup.
    known_pages: HashSet<String>,
    /// Last sync timestamp (epoch millis).
    last_sync_ms: i64,
    /// Timestamp to save on next flush.
    next_sync_ms: i64,
    /// Pending writes — flushed in one batch at the end.
    pending: Vec<PageEntry>,
}

impl SyncStore {
    pub fn open(db_dir: &Path) -> Result<Self> {
        let data_path = db_dir.join("data");
        let index_path = db_dir.join("index");

        std::fs::create_dir_all(&data_path)
            .map_err(|e| RoamError::Generic(format!("Failed to create data dir: {}", e)))?;
        std::fs::create_dir_all(&index_path)
            .map_err(|e| RoamError::Generic(format!("Failed to create index dir: {}", e)))?;

        #[allow(deprecated)]
        let db = match chrondb::ChronDB::open(
            data_path.to_str().unwrap_or(""),
            index_path.to_str().unwrap_or(""),
        ) {
            Ok(db) => db,
            Err(_) => {
                eprintln!("ChronDB: index corrupted, rebuilding...");
                let _ = std::fs::remove_dir_all(&index_path);
                let _ = std::fs::create_dir_all(&index_path);
                #[allow(deprecated)]
                chrondb::ChronDB::open(
                    data_path.to_str().unwrap_or(""),
                    index_path.to_str().unwrap_or(""),
                )
                .map_err(|e| RoamError::Generic(format!("Failed to open ChronDB: {}", e)))?
            }
        };

        // Load known pages from index
        let mut known_pages = HashSet::new();
        if let Ok(list) = db.list_by_prefix("page:", None) {
            if let Some(arr) = list.as_array() {
                for item in arr {
                    if let Some(key) = item.get("id").and_then(|v| v.as_str()) {
                        if let Some(uid) = key.strip_prefix("page:") {
                            known_pages.insert(uid.to_string());
                        }
                    }
                }
            } else if let Some(obj) = list.as_object() {
                for key in obj.keys() {
                    if let Some(uid) = key.strip_prefix("page:") {
                        known_pages.insert(uid.to_string());
                    }
                }
            }
        }

        let last_sync_ms = db
            .get("meta:last_sync_ms", None)
            .ok()
            .and_then(|v| v.get("ts")?.as_i64())
            .unwrap_or(0);

        Ok(Self {
            db,
            known_pages,
            last_sync_ms,
            next_sync_ms: 0,
            pending: Vec::new(),
        })
    }

    pub fn has_page(&self, uid: &str) -> bool {
        self.known_pages.contains(uid)
    }

    pub fn put_page(&mut self, uid: &str, title: &str, file_path: &str, block_count: usize) {
        self.known_pages.insert(uid.to_string());
        self.pending.push(PageEntry {
            uid: uid.to_string(),
            title: title.to_string(),
            file_path: file_path.to_string(),
            block_count,
        });
    }

    /// Flush pending pages + save timestamp + push to remote. Reuses the open connection.
    pub fn flush(&mut self) -> Result<()> {
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
                if let Err(e) = self.db.put(&key, &doc, None) {
                    eprintln!("ChronDB: failed to write page {}: {}", entry.uid, e);
                    break;
                }
            }
        }

        // Save sync timestamp
        if self.next_sync_ms > 0 {
            let _ = self
                .db
                .put("meta:last_sync_ms", &json!({"ts": self.next_sync_ms}), None);
            self.last_sync_ms = self.next_sync_ms;
        }

        let count = self.pending.len();
        self.pending.clear();
        if count > 0 {
            eprintln!("ChronDB: {} pages written", count);
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn page_history(&self, uid: &str) -> Result<Value> {
        let key = format!("page:{}", uid);
        self.db
            .history(&key, None)
            .map_err(|e| RoamError::Generic(format!("ChronDB history error: {}", e)))
    }

    pub fn known_page_count(&self) -> usize {
        self.known_pages.len()
    }

    pub fn last_sync_ms(&self) -> i64 {
        self.last_sync_ms
    }

    pub fn set_next_sync_ms(&mut self, ms: i64) {
        self.next_sync_ms = ms;
    }
}
