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

    // Stage all changes
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(sync_dir)
        .output();

    // Read staged changes to build commit message
    let changes = get_staged_changes(sync_dir);
    if changes.is_empty() {
        return; // nothing to commit or push
    }

    let message = build_commit_message(&changes);
    let _ = Command::new("git")
        .args(["commit", "-m", &message])
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

#[derive(Debug, Clone, PartialEq)]
enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

fn get_staged_changes(sync_dir: &Path) -> Vec<(ChangeKind, String)> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-status"])
        .current_dir(sync_dir)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '\t');
            let status = parts.next()?.trim();
            let path = parts.next()?.trim().to_string();
            let kind = match status.chars().next()? {
                'A' => ChangeKind::Added,
                'M' => ChangeKind::Modified,
                'D' => ChangeKind::Deleted,
                _ => ChangeKind::Modified,
            };
            Some((kind, path))
        })
        .collect()
}

fn build_commit_message(changes: &[(ChangeKind, String)]) -> String {
    let added: Vec<&str> = changes
        .iter()
        .filter(|(k, _)| *k == ChangeKind::Added)
        .map(|(_, p)| humanize_path(p))
        .collect();
    let modified: Vec<&str> = changes
        .iter()
        .filter(|(k, _)| *k == ChangeKind::Modified)
        .map(|(_, p)| humanize_path(p))
        .collect();
    let deleted: Vec<&str> = changes
        .iter()
        .filter(|(k, _)| *k == ChangeKind::Deleted)
        .map(|(_, p)| humanize_path(p))
        .collect();

    let title = build_title(&added, &modified, &deleted);
    let body = build_body(&added, &modified, &deleted);

    if body.is_empty() {
        title
    } else {
        format!("{}\n\n{}", title, body)
    }
}

fn build_title(added: &[&str], modified: &[&str], deleted: &[&str]) -> String {
    let total = added.len() + modified.len() + deleted.len();

    // Single operation type
    if modified.is_empty() && deleted.is_empty() {
        return format!("sync: add {}", summarize_names(added));
    }
    if added.is_empty() && deleted.is_empty() {
        return format!("sync: update {}", summarize_names(modified));
    }
    if added.is_empty() && modified.is_empty() {
        return format!("sync: remove {}", summarize_names(deleted));
    }

    // Mixed operations — aggregate
    let mut parts: Vec<String> = Vec::new();
    if !added.is_empty() {
        parts.push(count_by_type(added, "added"));
    }
    if !modified.is_empty() {
        parts.push(count_by_type(modified, "updated"));
    }
    if !deleted.is_empty() {
        parts.push(count_by_type(deleted, "removed"));
    }

    let detail = parts.join(", ");
    if total <= 3 {
        format!("sync: {}", detail)
    } else {
        format!("sync: {} changes ({})", total, detail)
    }
}

fn summarize_names(names: &[&str]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].to_string(),
        2 => format!("{} and {}", names[0], names[1]),
        3 => format!("{}, {} and {}", names[0], names[1], names[2]),
        n => {
            let (daily, pages): (Vec<&&str>, Vec<&&str>) =
                names.iter().partition(|n| n.starts_with("daily/"));
            let mut parts = Vec::new();
            if !daily.is_empty() {
                parts.push(format!("{} daily notes", daily.len()));
            }
            if !pages.is_empty() {
                parts.push(format!("{} pages", pages.len()));
            }
            if parts.is_empty() {
                format!("{} files", n)
            } else {
                parts.join(" and ")
            }
        }
    }
}

fn count_by_type(names: &[&str], verb: &str) -> String {
    if names.len() == 1 {
        format!("{} {}", names[0], verb)
    } else {
        format!("{} {}", names.len(), verb)
    }
}

fn build_body(added: &[&str], modified: &[&str], deleted: &[&str]) -> String {
    let total = added.len() + modified.len() + deleted.len();
    if total <= 3 {
        return String::new();
    }

    let mut lines = Vec::new();
    if !added.is_empty() {
        lines.push(format!("Added: {}", added.join(", ")));
    }
    if !modified.is_empty() {
        lines.push(format!("Updated: {}", modified.join(", ")));
    }
    if !deleted.is_empty() {
        lines.push(format!("Removed: {}", deleted.join(", ")));
    }
    lines.join("\n")
}

fn humanize_path(path: &str) -> &str {
    path.strip_suffix(".md").unwrap_or(path)
}

#[cfg(test)]
mod commit_tests {
    use super::*;

    #[test]
    fn single_page_added() {
        let changes = vec![(ChangeKind::Added, "pages/Project Alpha.md".into())];
        assert_eq!(
            build_commit_message(&changes),
            "sync: add pages/Project Alpha"
        );
    }

    #[test]
    fn single_page_modified() {
        let changes = vec![(ChangeKind::Modified, "pages/Meeting Notes.md".into())];
        assert_eq!(
            build_commit_message(&changes),
            "sync: update pages/Meeting Notes"
        );
    }

    #[test]
    fn single_page_deleted() {
        let changes = vec![(ChangeKind::Deleted, "pages/Old Page.md".into())];
        assert_eq!(
            build_commit_message(&changes),
            "sync: remove pages/Old Page"
        );
    }

    #[test]
    fn two_pages_modified() {
        let changes = vec![
            (ChangeKind::Modified, "pages/Alpha.md".into()),
            (ChangeKind::Modified, "pages/Beta.md".into()),
        ];
        assert_eq!(
            build_commit_message(&changes),
            "sync: update pages/Alpha and pages/Beta"
        );
    }

    #[test]
    fn daily_note_added() {
        let changes = vec![(ChangeKind::Added, "daily/04-03-2026.md".into())];
        assert_eq!(build_commit_message(&changes), "sync: add daily/04-03-2026");
    }

    #[test]
    fn mixed_operations_few() {
        let changes = vec![
            (ChangeKind::Added, "pages/New.md".into()),
            (ChangeKind::Modified, "daily/04-03-2026.md".into()),
        ];
        let msg = build_commit_message(&changes);
        assert!(msg.starts_with("sync:"));
        assert!(msg.contains("added"));
        assert!(msg.contains("updated"));
    }

    #[test]
    fn many_files_has_body() {
        let changes = vec![
            (ChangeKind::Added, "pages/A.md".into()),
            (ChangeKind::Added, "pages/B.md".into()),
            (ChangeKind::Modified, "pages/C.md".into()),
            (ChangeKind::Deleted, "pages/D.md".into()),
        ];
        let msg = build_commit_message(&changes);
        assert!(msg.contains("\n\n")); // has body
        assert!(msg.contains("Added:"));
        assert!(msg.contains("Updated:"));
        assert!(msg.contains("Removed:"));
    }

    #[test]
    fn many_same_type_aggregates() {
        let changes: Vec<_> = (0..10)
            .map(|i| (ChangeKind::Added, format!("pages/Page{}.md", i)))
            .collect();
        let msg = build_commit_message(&changes);
        assert!(msg.contains("10 pages"));
    }

    #[test]
    fn empty_changes_no_crash() {
        let msg = build_commit_message(&[]);
        assert!(msg.is_empty() || msg.starts_with("sync:"));
    }
}
