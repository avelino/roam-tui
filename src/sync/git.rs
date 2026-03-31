use std::path::Path;
use std::process::Command;

/// Initialize the sync output dir as a git repo if needed, pull, and configure remote.
pub fn pull(sync_dir: &Path, remote: &str) {
    if !sync_dir.exists() {
        return;
    }

    init_if_needed(sync_dir);
    ensure_remote(sync_dir, remote);

    eprintln!("Git: pulling from remote...");
    match Command::new("git")
        .args(["pull", "--rebase", "origin", "main"])
        .current_dir(sync_dir)
        .output()
    {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Not fatal — remote might be empty or first sync
            if !stderr.contains("Couldn't find remote ref") {
                eprintln!("Git: pull warning: {}", stderr.trim());
            }
        }
        Err(e) => eprintln!("Git: pull failed: {}", e),
        _ => {}
    }
}

/// Commit all changes and push to remote.
pub fn commit_and_push(sync_dir: &Path, remote: &str) {
    if !sync_dir.join(".git").exists() {
        init_if_needed(sync_dir);
    }

    ensure_remote(sync_dir, remote);

    // Stage all changes
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(sync_dir)
        .output();

    // Commit (might be empty if nothing changed — that's ok)
    let _ = Command::new("git")
        .args(["commit", "-m", "sync: update from Roam"])
        .current_dir(sync_dir)
        .output();

    // Push
    eprintln!("Git: pushing to remote...");
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

fn init_if_needed(dir: &Path) {
    if !dir.join(".git").exists() {
        let _ = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(dir)
            .output();
    }
}

fn ensure_remote(dir: &Path, remote: &str) {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let current = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if current != remote {
                let _ = Command::new("git")
                    .args(["remote", "set-url", "origin", remote])
                    .current_dir(dir)
                    .output();
            }
        }
        _ => {
            let _ = Command::new("git")
                .args(["remote", "add", "origin", remote])
                .current_dir(dir)
                .output();
        }
    }
}
