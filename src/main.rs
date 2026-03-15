// Re-export SDK modules so binary-internal modules can use crate::api:: and crate::error::
pub(crate) use roam_sdk::{api, error};

mod app;
mod config;
mod edit_buffer;
mod export;
mod highlight;
mod keys;
mod markdown;
mod mcp;
mod ui;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use config::AppConfig;

#[derive(Parser)]
#[command(name = "roam", about = "Roam Research TUI and MCP server")]
struct Cli {
    /// Run as MCP server (stdio transport)
    #[arg(long)]
    mcp: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Export daily notes or pages to JSON or Markdown
    Export {
        /// Date in YYYY-MM-DD format (default: today)
        #[arg(long)]
        date: Option<String>,

        /// Page title to export (if set, exports a page instead of daily note)
        #[arg(long)]
        page: Option<String>,

        /// Output format: "md" or "json" (default: md)
        #[arg(long, default_value = "md")]
        format: String,

        /// Output file path (default: stdout)
        #[arg(long, short)]
        output: Option<String>,
    },
}

async fn run_export(
    config: &AppConfig,
    date: Option<String>,
    page: Option<String>,
    format: &str,
    output: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    use chrono::{Datelike, Local, NaiveDate};
    use roam_sdk::api::client::RoamClient;
    use roam_sdk::api::queries;
    use roam_sdk::api::types::DailyNote;

    let client = RoamClient::new(&config.graph.name, &config.graph.api_token);

    let content = if let Some(title) = page {
        let (eid, selector) = queries::pull_page_by_title(&title);
        let resp = client.pull(eid, &selector).await?;
        let note =
            DailyNote::from_pull_response(Local::now().date_naive(), "".into(), &resp.result);

        match format {
            "json" => export::blocks_to_json(&title, &note.blocks),
            _ => export::blocks_to_markdown(&title, &note.blocks),
        }
    } else {
        let date = match date {
            Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")?,
            None => Local::now().date_naive(),
        };
        let uid = queries::daily_note_uid_for_date(date.month(), date.day(), date.year());
        let (eid, selector) = queries::pull_daily_note(&uid);
        let resp = client.pull(eid, &selector).await?;
        let note = DailyNote::from_pull_response(date, uid, &resp.result);

        match format {
            "json" => export::daily_notes_to_json(&[note]),
            _ => export::daily_notes_to_markdown(&[note]),
        }
    };

    if let Some(path) = output {
        std::fs::write(&path, &content)?;
        eprintln!("Exported to {}", path);
    } else {
        print!("{}", content);
    }

    Ok(())
}

fn config_path() -> PathBuf {
    AppConfig::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.toml")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
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

    if cli.mcp {
        mcp::run(&config).await?;
        return Ok(());
    }

    if let Some(Commands::Export {
        date,
        page,
        format,
        output,
    }) = cli.command
    {
        run_export(&config, date, page, &format, output).await?;
        return Ok(());
    }

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
