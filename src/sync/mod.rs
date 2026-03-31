pub mod fs;
pub mod pull;
pub mod push;
pub mod store;

use std::path::PathBuf;

use crate::api::client::RoamClient;
use crate::error::Result;

use self::store::SyncStore;

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum SyncDirection {
    Pull,
    Push,
    Both,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SyncConfig {
    pub direction: SyncDirection,
    pub output_dir: PathBuf,
    pub include_daily_notes: bool,
    pub dry_run: bool,
    pub concurrency: usize,
    pub filter: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            direction: SyncDirection::Pull,
            output_dir: PathBuf::from("roam-sync"),
            include_daily_notes: false,
            dry_run: false,
            concurrency: 5,
            filter: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub deleted: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for SyncReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Sync complete: {} created, {} updated, {} unchanged, {} deleted",
            self.created, self.updated, self.unchanged, self.deleted
        )?;
        if !self.errors.is_empty() {
            write!(f, ", {} errors", self.errors.len())?;
        }
        Ok(())
    }
}

pub struct SyncEngine {
    client: RoamClient,
    store: SyncStore,
    config: SyncConfig,
}

impl SyncEngine {
    pub fn new(client: RoamClient, config: SyncConfig) -> Result<Self> {
        let db_dir = config.output_dir.join(".chrondb");
        let store = SyncStore::open(&db_dir)?;
        Ok(Self {
            client,
            store,
            config,
        })
    }

    pub async fn run(&self) -> Result<SyncReport> {
        match self.config.direction {
            SyncDirection::Pull => pull::pull_sync(&self.client, &self.store, &self.config).await,
            SyncDirection::Push => push::push_sync(&self.client, &self.store, &self.config).await,
            SyncDirection::Both => {
                let mut report = pull::pull_sync(&self.client, &self.store, &self.config).await?;
                let push_report = push::push_sync(&self.client, &self.store, &self.config).await?;
                report.created += push_report.created;
                report.updated += push_report.updated;
                report.errors.extend(push_report.errors);
                Ok(report)
            }
        }
    }

    pub fn history(&self, block_uid: &str) -> Result<serde_json::Value> {
        self.store.history(block_uid)
    }
}
