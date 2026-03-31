use std::path::{Path, PathBuf};

pub fn sanitize_filename(title: &str) -> String {
    title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string()
}

pub fn page_path(base: &Path, title: &str) -> PathBuf {
    base.join("pages")
        .join(format!("{}.md", sanitize_filename(title)))
}

/// Daily note path from UID (MM-DD-YYYY → daily/MM-DD-YYYY.md).
pub fn daily_path_from_uid(base: &Path, uid: &str) -> PathBuf {
    base.join("daily").join(format!("{}.md", uid))
}

pub fn ensure_dirs(base: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(base.join("pages"))?;
    std::fs::create_dir_all(base.join("daily"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_special_chars() {
        assert_eq!(sanitize_filename("C++ / Rust"), "C++ _ Rust");
        assert_eq!(sanitize_filename("What?"), "What_");
        assert_eq!(sanitize_filename("a:b:c"), "a_b_c");
        assert_eq!(sanitize_filename("pipe|here"), "pipe_here");
    }

    #[test]
    fn sanitize_trims_whitespace() {
        assert_eq!(sanitize_filename("  spaced  "), "spaced");
    }

    #[test]
    fn sanitize_preserves_normal_chars() {
        assert_eq!(sanitize_filename("My Page Title"), "My Page Title");
        assert_eq!(sanitize_filename("2026-03-30"), "2026-03-30");
    }

    #[test]
    fn page_path_format() {
        let base = Path::new("/tmp/roam-sync");
        let p = page_path(base, "My Page");
        assert_eq!(p, PathBuf::from("/tmp/roam-sync/pages/My Page.md"));
    }

    #[test]
    fn daily_path_from_uid_format() {
        let base = Path::new("/tmp/roam-sync");
        let p = daily_path_from_uid(base, "03-30-2026");
        assert_eq!(p, PathBuf::from("/tmp/roam-sync/daily/03-30-2026.md"));
    }
}
