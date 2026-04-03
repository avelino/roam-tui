pub mod fs;
pub mod parse;
pub mod pull;
pub mod push;
pub mod store;

use std::path::{Path, PathBuf};
use std::process::Command;

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

        // Push markdown files to remote (always when configured)
        if !self.config.remote.is_empty() {
            git_commit_and_push(&self.config.output_dir, &self.config.remote);
        }

        Ok(report)
    }

    pub fn history(&self, page_uid: &str) -> Option<serde_json::Value> {
        self.store.page_history(page_uid).ok()
    }
}

// --- Git operations on the markdown sync directory ---

fn git_commit_and_push(sync_dir: &Path, remote: &str) {
    if !sync_dir.exists() {
        return;
    }

    // Init if needed
    if !sync_dir.join(".git").exists() {
        let _ = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(sync_dir)
            .output();
    }

    // Ensure remote
    let has_remote = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(sync_dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_remote {
        let _ = Command::new("git")
            .args(["remote", "set-url", "origin", remote])
            .current_dir(sync_dir)
            .output();
    } else {
        let _ = Command::new("git")
            .args(["remote", "add", "origin", remote])
            .current_dir(sync_dir)
            .output();
    }

    // Stage + commit + push
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(sync_dir)
        .output();

    let _ = Command::new("git")
        .args(["commit", "-m", "sync: update from Roam"])
        .current_dir(sync_dir)
        .output();

    eprintln!("Git: pushing markdowns to remote...");
    match Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(sync_dir)
        .output()
    {
        Ok(output) if output.status.success() => {
            eprintln!("Git: pushed to remote");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Git: push warning: {}", stderr.trim());
        }
        Err(e) => eprintln!("Git: push failed: {}", e),
    }
}
