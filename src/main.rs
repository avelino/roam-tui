// Re-export SDK modules so binary-internal modules can use crate::api:: and crate::error::
pub(crate) use roam_sdk::{api, error};

mod app;
mod config;
mod edit_buffer;
mod highlight;
mod keys;
mod markdown;
mod ui;

use std::path::PathBuf;

use config::AppConfig;

fn config_path() -> PathBuf {
    AppConfig::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.toml")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();

    if !path.exists() {
        AppConfig::write_default(&path)?;
        eprintln!(
            "Created default config at: {}\nPlease edit it with your Roam graph name and API token, then run again.",
            path.display()
        );
        return Ok(());
    }

    let config = match AppConfig::load_from_path(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config from {}: {}", path.display(), e);
            eprintln!("Fix the config file or delete it to regenerate defaults.");
            return Ok(());
        }
    };

    let mut terminal = ratatui::init();

    // Enable enhanced keyboard protocol (reports Cmd/Super on supported terminals)
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    );

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::PopKeyboardEnhancementFlags
        );
        ratatui::restore();
        hook(info);
    }));

    let result = app::run(&config, &mut terminal).await;

    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::PopKeyboardEnhancementFlags
    );
    ratatui::restore();

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}
