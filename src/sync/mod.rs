pub mod fs;
pub mod git;
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
    pub db_dir: PathBuf,
    pub remote: String,
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
            db_dir: PathBuf::from(".chrondb"),
            remote: String::new(),
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
        // Pull markdown files from remote before starting
        if !config.remote.is_empty() {
            git::pull(&config.output_dir, &config.remote);
        }

        let store = SyncStore::open(&config.db_dir)?;
        eprintln!("{} pages in ChronDB", store.known_page_count());
        Ok(Self {
            client,
            store,
            config,
        })
    }

    pub async fn run(&mut self) -> Result<SyncReport> {
        let report = match self.config.direction {
            SyncDirection::Pull => {
                pull::pull_sync(&self.client, &mut self.store, &self.config).await?
            }
            SyncDirection::Push => {
                push::push_sync(&self.client, &mut self.store, &self.config).await?
            }
            SyncDirection::Both => {
                let mut report =
                    pull::pull_sync(&self.client, &mut self.store, &self.config).await?;
                let push_report =
                    push::push_sync(&self.client, &mut self.store, &self.config).await?;
                report.created += push_report.created;
                report.updated += push_report.updated;
                report.errors.extend(push_report.errors);
                report
            }
        };

        // Always commit + push markdown files to remote
        if !self.config.remote.is_empty() {
            git::commit_and_push(&self.config.output_dir, &self.config.remote);
        }

        Ok(report)
    }

    pub fn history(&self, page_uid: &str) -> Option<serde_json::Value> {
        self.store.page_history(page_uid).ok()
    }
}
