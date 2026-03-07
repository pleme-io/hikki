mod config;
mod platform;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::config::HikkiConfig;

#[derive(Parser)]
#[command(name = "hikki", about = "Hikki (筆記) — GPU notes app")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Search notes by query.
    Search {
        /// Search query string.
        query: String,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Load config via shikumi
    let config = match shikumi::ConfigDiscovery::new("hikki")
        .env_override("HIKKI_CONFIG")
        .discover()
    {
        Ok(path) => {
            tracing::info!("loading config from {}", path.display());
            let store =
                shikumi::ConfigStore::<HikkiConfig>::load(&path, "HIKKI_").unwrap_or_else(|e| {
                    tracing::warn!("failed to load config: {e}, using defaults");
                    let tmp = std::env::temp_dir().join("hikki-default.yaml");
                    std::fs::write(&tmp, "{}").ok();
                    shikumi::ConfigStore::load(&tmp, "HIKKI_").unwrap()
                });
            HikkiConfig::clone(&store.get())
        }
        Err(_) => {
            tracing::info!("no config file found, using defaults");
            HikkiConfig::default()
        }
    };

    match cli.command {
        Some(Command::Search { query }) => {
            tracing::info!("searching notes for: {query}");
            let storage = platform::create_storage(&config.storage.notes_dir);
            match storage.search_notes(&query) {
                Ok(results) => {
                    if results.is_empty() {
                        println!("No notes found matching: {query}");
                    } else {
                        for note in &results {
                            println!("{}: {}", note.id, note.title);
                        }
                        println!("\n{} note(s) found.", results.len());
                    }
                }
                Err(e) => {
                    tracing::error!("search failed: {e}");
                }
            }
        }
        None => {
            tracing::info!("launching hikki GUI");
            tracing::info!(
                "notes dir: {}",
                config.storage.notes_dir.display()
            );
            // GUI event loop will be implemented here
        }
    }
}
