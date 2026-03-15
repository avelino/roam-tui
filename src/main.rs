// Re-export SDK modules so binary-internal modules can use crate::api:: and crate::error::
pub(crate) use roam_sdk::{api, error};

mod app;
mod commands;
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
use roam_sdk::api::client::RoamClient;

#[derive(Parser)]
#[command(name = "roam", about = "Roam Research TUI and MCP server")]
struct Cli {
    /// Run as MCP server (stdio transport)
    #[arg(long, hide = true)]
    mcp: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run as MCP server (stdio transport)
    Mcp,

    /// Journal — view or add to today's daily note (alias: j)
    #[command(alias = "j")]
    Journal {
        #[command(subcommand)]
        action: Option<JournalAction>,
    },

    /// Search pages or blocks
    Search {
        /// Search query string
        query: String,

        /// Search inside block content instead of page titles
        #[arg(long, short)]
        blocks: bool,

        /// Maximum number of results
        #[arg(long, short)]
        limit: Option<usize>,
    },

    /// Get a resource (page, block, daily note, backlinks, refs, stats)
    Get {
        #[command(subcommand)]
        resource: GetResource,
    },

    /// Run a raw Datalog query
    Query {
        /// Datalog query string
        query: String,

        /// JSON array of query arguments
        #[arg(long)]
        args: Option<String>,
    },

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

    /// Create a page or block
    Create {
        #[command(subcommand)]
        resource: CreateResource,
    },

    /// Update a block's content
    Update {
        #[command(subcommand)]
        resource: UpdateResource,
    },

    /// Delete a block or page
    Delete {
        #[command(subcommand)]
        resource: DeleteResource,
    },

    /// Move a block to a new parent
    Move {
        #[command(subcommand)]
        resource: MoveResource,
    },

    /// Execute batch write operations from a JSON file or stdin
    Batch {
        /// JSON file path (use "-" for stdin)
        #[arg(default_value = "-")]
        file: String,
    },
}

#[derive(Subcommand)]
enum JournalAction {
    /// View today's (or a specific date's) daily note
    View {
        /// Date in YYYY-MM-DD format (default: today)
        #[arg(long)]
        date: Option<String>,
    },
    /// Add a block to the daily note
    Add {
        /// Block content
        text: String,

        /// Date in YYYY-MM-DD format (default: today)
        #[arg(long)]
        date: Option<String>,

        /// Position: "first", "last", or numeric index
        #[arg(long)]
        order: Option<String>,

        /// JSON array of child block strings
        #[arg(long)]
        children: Option<String>,
    },
}

#[derive(Subcommand)]
enum GetResource {
    /// Get a page by title
    Page {
        /// Page title
        title: String,
    },
    /// Get a block by UID
    Block {
        /// Block UID
        uid: String,
    },
    /// Get a daily note
    Daily {
        /// Date in YYYY-MM-DD format (default: today)
        #[arg(long)]
        date: Option<String>,
    },
    /// Get backlinks for a page
    Backlinks {
        /// Page title
        title: String,
    },
    /// Get outbound references from a block
    Refs {
        /// Block UID
        uid: String,
    },
    /// Get graph statistics (page and block counts)
    Stats,
}

#[derive(Subcommand)]
enum CreateResource {
    /// Create a new page
    Page {
        /// Page title
        title: String,

        /// Optional page UID
        #[arg(long)]
        uid: Option<String>,
    },
    /// Create a new block
    Block {
        /// Parent block or page UID
        #[arg(long)]
        parent: String,

        /// Block content
        content: String,

        /// Position: "first", "last", or numeric index
        #[arg(long)]
        order: Option<String>,

        /// Optional block UID
        #[arg(long)]
        uid: Option<String>,

        /// JSON array of child block strings
        #[arg(long)]
        children: Option<String>,
    },
}

#[derive(Subcommand)]
enum UpdateResource {
    /// Update a block's content
    Block {
        /// Block UID
        uid: String,

        /// New content
        content: String,
    },
}

#[derive(Subcommand)]
enum DeleteResource {
    /// Delete a block by UID
    Block {
        /// Block UID
        uid: String,
    },
    /// Delete a page by UID
    Page {
        /// Page UID
        uid: String,
    },
}

#[derive(Subcommand)]
enum MoveResource {
    /// Move a block to a new parent
    Block {
        /// Block UID to move
        uid: String,

        /// New parent UID
        #[arg(long)]
        parent: String,

        /// Position: "first", "last", or numeric index
        #[arg(long)]
        order: Option<String>,
    },
}

fn config_path() -> PathBuf {
    AppConfig::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.toml")
}

fn make_client(config: &AppConfig) -> RoamClient {
    RoamClient::new(&config.graph.name, &config.graph.api_token)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = Cli::parse();
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

    // Handle --mcp flag (backward compat, hidden)
    if cli_args.mcp {
        mcp::run(&config).await?;
        return Ok(());
    }

    if let Some(command) = cli_args.command {
        let client = make_client(&config);
        let result = match command {
            Commands::Mcp => {
                mcp::run(&config).await?;
                return Ok(());
            }
            Commands::Journal { action } => match action {
                None => commands::journal_view(&client, None).await,
                Some(JournalAction::View { date }) => {
                    commands::journal_view(&client, date.as_deref()).await
                }
                Some(JournalAction::Add {
                    text,
                    date,
                    order,
                    children,
                }) => {
                    commands::journal_add(
                        &client,
                        &text,
                        date.as_deref(),
                        order.as_deref(),
                        children.as_deref(),
                    )
                    .await
                }
            },
            Commands::Search {
                query,
                blocks,
                limit,
            } => commands::search(&client, &query, blocks, limit).await,
            Commands::Get { resource } => match resource {
                GetResource::Page { title } => commands::get_page(&client, &title).await,
                GetResource::Block { uid } => commands::get_block(&client, &uid).await,
                GetResource::Daily { date } => commands::get_daily(&client, date.as_deref()).await,
                GetResource::Backlinks { title } => commands::get_backlinks(&client, &title).await,
                GetResource::Refs { uid } => commands::get_refs(&client, &uid).await,
                GetResource::Stats => commands::get_stats(&client).await,
            },
            Commands::Query { query, args } => {
                commands::query(&client, &query, args.as_deref()).await
            }
            Commands::Export {
                date,
                page,
                format,
                output,
            } => {
                commands::run_export(
                    &client,
                    date.as_deref(),
                    page.as_deref(),
                    &format,
                    output.as_deref(),
                )
                .await?;
                return Ok(());
            }
            Commands::Create { resource } => match resource {
                CreateResource::Page { title, uid } => {
                    commands::create_page(&client, &title, uid.as_deref()).await
                }
                CreateResource::Block {
                    parent,
                    content,
                    order,
                    uid,
                    children,
                } => {
                    commands::create_block(
                        &client,
                        &parent,
                        &content,
                        order.as_deref(),
                        uid.as_deref(),
                        children.as_deref(),
                    )
                    .await
                }
            },
            Commands::Update { resource } => match resource {
                UpdateResource::Block { uid, content } => {
                    commands::update_block(&client, &uid, &content).await
                }
            },
            Commands::Delete { resource } => match resource {
                DeleteResource::Block { uid } => commands::delete_block(&client, &uid).await,
                DeleteResource::Page { uid } => commands::delete_page(&client, &uid).await,
            },
            Commands::Move { resource } => match resource {
                MoveResource::Block { uid, parent, order } => {
                    commands::move_block(&client, &uid, &parent, order.as_deref()).await
                }
            },
            Commands::Batch { file } => {
                let input = if file == "-" {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                } else {
                    std::fs::read_to_string(&file)?
                };
                commands::batch(&client, &input).await
            }
        };

        match result {
            Ok(output) => {
                println!("{}", output);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        return Ok(());
    }

    // No subcommand — launch TUI
    let mut terminal = ratatui::init();

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
