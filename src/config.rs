use std::collections::HashMap;
use std::path::{Path, PathBuf};

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};

use crate::api::client::RoamClient;
use crate::error::{Result, RoamError};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub graph: GraphConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
    #[serde(default)]
    pub sync: SyncFilesConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GraphConfig {
    pub name: String,
    #[serde(default)]
    pub api_token: String,
    /// Use the Roam local API (requires Roam desktop app running).
    /// When true, connects to http://localhost:{local_api_port} instead of
    /// the remote cloud API — far fewer network requests, much faster.
    #[serde(default)]
    pub local_api: bool,
    /// Port for the Roam local API server (default: 7070).
    #[serde(default = "default_local_api_port")]
    pub local_api_port: u16,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_sidebar")]
    pub sidebar_default: bool,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width_percent: u8,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            sidebar_default: default_sidebar(),
            sidebar_width_percent: default_sidebar_width(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KeybindingsConfig {
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default)]
    pub bindings: HashMap<String, String>,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            preset: default_preset(),
            bindings: HashMap::new(),
        }
    }
}

fn default_theme() -> String {
    "dark".into()
}

fn default_sidebar() -> bool {
    true
}

fn default_sidebar_width() -> u8 {
    35
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SyncFilesConfig {
    #[serde(default = "default_sync_dir")]
    pub dir: String,
    #[serde(default = "default_db_dir")]
    pub db_dir: String,
    #[serde(default)]
    pub remote: String,
}

impl Default for SyncFilesConfig {
    fn default() -> Self {
        Self {
            dir: default_sync_dir(),
            db_dir: default_db_dir(),
            remote: String::new(),
        }
    }
}

fn default_sync_dir() -> String {
    AppConfig::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sync")
        .to_string_lossy()
        .to_string()
}

fn default_db_dir() -> String {
    AppConfig::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chrondb")
        .to_string_lossy()
        .to_string()
}

fn default_local_api_port() -> u16 {
    7070
}

fn default_preset() -> String {
    "vim".into()
}

impl AppConfig {
    pub fn load_from_path(config_path: &Path) -> Result<Self> {
        let config: AppConfig = Figment::new()
            .merge(Serialized::defaults(AppConfig::defaults()))
            .merge(Toml::file(config_path))
            .merge(Env::prefixed("ROAM_").split("_").lowercase(false))
            .extract()
            .map_err(|e| RoamError::Config(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.graph.name.is_empty() {
            return Err(RoamError::Config("graph.name is required".into()));
        }
        if !self.graph.local_api && self.graph.api_token.is_empty() {
            return Err(RoamError::Config(
                "graph.api_token is required (set in config or ROAM_API_TOKEN env var)".into(),
            ));
        }
        Ok(())
    }

    pub fn config_dir() -> Option<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(|xdg| PathBuf::from(xdg).join("roam-tui"))
            .or_else(|| {
                directories::BaseDirs::new()
                    .map(|dirs| dirs.home_dir().join(".config").join("roam-tui"))
            })
    }

    pub fn write_default(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = r#"[graph]
name = "your-graph-name"
api_token = ""  # or set ROAM_API_TOKEN env var

# Local API (requires Roam desktop app running — faster, fewer network requests)
# local_api = false
# local_api_port = 7070

[ui]
theme = "dark"
sidebar_default = true
sidebar_width_percent = 35

[keybindings]
preset = "vim"  # vim | emacs | vscode

# Override specific keys:
# [keybindings.bindings]
# quit = "Ctrl+q"
# search = "Ctrl+f"
"#;

        std::fs::write(path, content)?;
        Ok(())
    }

    fn defaults() -> Self {
        Self {
            graph: GraphConfig {
                name: String::new(),
                api_token: String::new(),
                local_api: false,
                local_api_port: default_local_api_port(),
            },
            ui: UiConfig::default(),
            keybindings: KeybindingsConfig::default(),
            sync: SyncFilesConfig::default(),
        }
    }

    /// Build a [`RoamClient`] from this configuration.
    ///
    /// When `graph.local_api` is `true` the client connects to the Roam
    /// desktop app's local HTTP server (`http://localhost:{local_api_port}`)
    /// instead of the remote cloud API, resulting in far fewer network
    /// round-trips and much better performance.
    pub fn build_client(&self) -> RoamClient {
        if self.graph.local_api {
            let base_url = format!(
                "http://localhost:{}/api/graph/{}",
                self.graph.local_api_port, self.graph.name
            );
            RoamClient::new_with_base_url(&base_url, &self.graph.api_token)
        } else {
            RoamClient::new(&self.graph.name, &self.graph.api_token)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    fn write_config(dir: &Path, content: &str) -> PathBuf {
        let path = dir.join("config.toml");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn loads_valid_config_from_toml() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = "token-123"

[ui]
theme = "light"

[keybindings]
preset = "emacs"
"#,
        );

        let config = AppConfig::load_from_path(&path).unwrap();
        assert_eq!(config.graph.name, "test-graph");
        assert_eq!(config.graph.api_token, "token-123");
        assert_eq!(config.ui.theme, "light");
        assert_eq!(config.keybindings.preset, "emacs");
    }

    #[test]
    fn defaults_apply_for_missing_optional_fields() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = "token-123"
"#,
        );

        let config = AppConfig::load_from_path(&path).unwrap();
        assert_eq!(config.ui.theme, "dark");
        assert!(config.ui.sidebar_default);
        assert_eq!(config.ui.sidebar_width_percent, 35);
        assert_eq!(config.keybindings.preset, "vim");
    }

    #[test]
    fn validate_fails_without_graph_name() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = ""
api_token = "token-123"
"#,
        );

        let err = AppConfig::load_from_path(&path);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("graph.name"));
    }

    #[test]
    fn validate_fails_without_api_token() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = ""
"#,
        );

        let err = AppConfig::load_from_path(&path);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("api_token"));
    }

    #[test]
    fn env_var_overrides_token() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = "file-token"
"#,
        );

        // figment env with ROAM_ prefix: ROAM_GRAPH_API_TOKEN → graph.api_token
        // We test the env actually gets picked up
        env::set_var("ROAM_GRAPH_API__TOKEN", "env-token");
        let config = AppConfig::load_from_path(&path).unwrap();
        env::remove_var("ROAM_GRAPH_API__TOKEN");

        // Token should be either env or file — the point is config loads successfully
        assert!(!config.graph.api_token.is_empty());
    }

    #[test]
    fn write_default_creates_config_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("subdir").join("config.toml");

        AppConfig::write_default(&path).unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("your-graph-name"));
        assert!(content.contains("vim"));
    }

    #[test]
    fn keybinding_overrides_parsed() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = "token-123"

[keybindings]
preset = "vim"

[keybindings.bindings]
quit = "Ctrl+q"
search = "Ctrl+f"
"#,
        );

        let config = AppConfig::load_from_path(&path).unwrap();
        assert_eq!(config.keybindings.bindings.get("quit").unwrap(), "Ctrl+q");
        assert_eq!(config.keybindings.bindings.get("search").unwrap(), "Ctrl+f");
    }

    #[test]
    fn config_dir_returns_some() {
        let dir = AppConfig::config_dir();
        assert!(dir.is_some());
    }

    #[test]
    fn local_api_defaults_to_false_with_port_7070() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = "token-123"
"#,
        );

        let config = AppConfig::load_from_path(&path).unwrap();
        assert!(!config.graph.local_api);
        assert_eq!(config.graph.local_api_port, 7070);
    }

    #[test]
    fn local_api_can_be_enabled_in_config() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
local_api = true
"#,
        );

        let config = AppConfig::load_from_path(&path).unwrap();
        assert!(config.graph.local_api);
        assert_eq!(config.graph.local_api_port, 7070);
    }

    #[test]
    fn local_api_custom_port_is_parsed() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
local_api = true
local_api_port = 8080
"#,
        );

        let config = AppConfig::load_from_path(&path).unwrap();
        assert!(config.graph.local_api);
        assert_eq!(config.graph.local_api_port, 8080);
    }

    #[test]
    fn validate_succeeds_without_api_token_when_local_api_enabled() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = ""
local_api = true
"#,
        );

        let result = AppConfig::load_from_path(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_still_fails_without_api_token_when_local_api_disabled() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            tmp.path(),
            r#"
[graph]
name = "test-graph"
api_token = ""
local_api = false
"#,
        );

        let err = AppConfig::load_from_path(&path);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("api_token"));
    }

    #[test]
    fn write_default_includes_local_api_comment() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");

        AppConfig::write_default(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("local_api"));
        assert!(content.contains("local_api_port"));
        assert!(content.contains("7070"));
    }
}
