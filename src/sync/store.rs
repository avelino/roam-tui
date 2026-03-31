use std::path::Path;

use serde_json::Value;

use crate::error::{Result, RoamError};

pub struct SyncStore {
    #[cfg(feature = "sync")]
    db: chrondb::ChronDB,
}

#[allow(dead_code)]
impl SyncStore {
    pub fn open(db_dir: &Path) -> Result<Self> {
        #[cfg(feature = "sync")]
        {
            let data_path = db_dir.join("data");
            let index_path = db_dir.join("index");

            std::fs::create_dir_all(&data_path).map_err(|e| {
                RoamError::Generic(format!("Failed to create ChronDB data dir: {}", e))
            })?;
            std::fs::create_dir_all(&index_path).map_err(|e| {
                RoamError::Generic(format!("Failed to create ChronDB index dir: {}", e))
            })?;

            let db = chrondb::ChronDB::open(
                data_path.to_str().unwrap_or(""),
                index_path.to_str().unwrap_or(""),
            )
            .map_err(|e| RoamError::Generic(format!("Failed to open ChronDB: {}", e)))?;

            Ok(Self { db })
        }

        #[cfg(not(feature = "sync"))]
        {
            let _ = db_dir;
            Err(RoamError::Generic(
                "Sync feature is not enabled. Rebuild with --features sync".to_string(),
            ))
        }
    }

    pub fn get_block(&self, uid: &str) -> Result<Option<Value>> {
        #[cfg(feature = "sync")]
        {
            let key = format!("block:{}", uid);
            match self.db.get(&key, None) {
                Ok(val) => Ok(Some(val)),
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("NotFound") || msg.contains("not found") {
                        Ok(None)
                    } else {
                        Err(RoamError::Generic(format!("ChronDB get error: {}", e)))
                    }
                }
            }
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = uid;
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn put_block(&self, uid: &str, doc: &Value) -> Result<()> {
        #[cfg(feature = "sync")]
        {
            let key = format!("block:{}", uid);
            self.db
                .put(&key, doc, None)
                .map_err(|e| RoamError::Generic(format!("ChronDB put error: {}", e)))?;
            Ok(())
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = (uid, doc);
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn delete_block(&self, uid: &str) -> Result<()> {
        #[cfg(feature = "sync")]
        {
            let key = format!("block:{}", uid);
            let _ = self.db.delete(&key, None);
            Ok(())
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = uid;
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn get_page(&self, uid: &str) -> Result<Option<Value>> {
        #[cfg(feature = "sync")]
        {
            let key = format!("page:{}", uid);
            match self.db.get(&key, None) {
                Ok(val) => Ok(Some(val)),
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("NotFound") || msg.contains("not found") {
                        Ok(None)
                    } else {
                        Err(RoamError::Generic(format!("ChronDB get error: {}", e)))
                    }
                }
            }
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = uid;
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn put_page(&self, uid: &str, doc: &Value) -> Result<()> {
        #[cfg(feature = "sync")]
        {
            let key = format!("page:{}", uid);
            self.db
                .put(&key, doc, None)
                .map_err(|e| RoamError::Generic(format!("ChronDB put error: {}", e)))?;
            Ok(())
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = (uid, doc);
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn delete_page(&self, uid: &str) -> Result<()> {
        #[cfg(feature = "sync")]
        {
            let key = format!("page:{}", uid);
            let _ = self.db.delete(&key, None);
            Ok(())
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = uid;
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn get_manifest(&self) -> Result<Option<Value>> {
        #[cfg(feature = "sync")]
        {
            match self.db.get("meta:manifest", None) {
                Ok(val) => Ok(Some(val)),
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("NotFound") || msg.contains("not found") {
                        Ok(None)
                    } else {
                        Err(RoamError::Generic(format!("ChronDB get error: {}", e)))
                    }
                }
            }
        }
        #[cfg(not(feature = "sync"))]
        Err(RoamError::Generic("Sync feature not enabled".to_string()))
    }

    pub fn put_manifest(&self, doc: &Value) -> Result<()> {
        #[cfg(feature = "sync")]
        {
            self.db
                .put("meta:manifest", doc, None)
                .map_err(|e| RoamError::Generic(format!("ChronDB put error: {}", e)))?;
            Ok(())
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = doc;
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn history(&self, block_uid: &str) -> Result<Value> {
        #[cfg(feature = "sync")]
        {
            let key = format!("block:{}", block_uid);
            self.db
                .history(&key, None)
                .map_err(|e| RoamError::Generic(format!("ChronDB history error: {}", e)))
        }
        #[cfg(not(feature = "sync"))]
        {
            let _ = block_uid;
            Err(RoamError::Generic("Sync feature not enabled".to_string()))
        }
    }

    pub fn list_blocks(&self) -> Result<Value> {
        #[cfg(feature = "sync")]
        {
            self.db
                .list_by_prefix("block:", None)
                .map_err(|e| RoamError::Generic(format!("ChronDB list error: {}", e)))
        }
        #[cfg(not(feature = "sync"))]
        Err(RoamError::Generic("Sync feature not enabled".to_string()))
    }

    pub fn list_pages(&self) -> Result<Value> {
        #[cfg(feature = "sync")]
        {
            self.db
                .list_by_prefix("page:", None)
                .map_err(|e| RoamError::Generic(format!("ChronDB list error: {}", e)))
        }
        #[cfg(not(feature = "sync"))]
        Err(RoamError::Generic("Sync feature not enabled".to_string()))
    }
}
