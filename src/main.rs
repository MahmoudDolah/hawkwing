use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use hawkwing::db::Database;
use hawkwing::scanner::scan_directory;

#[cfg(feature = "audio")]
use hawkwing::audio::Player;
#[cfg(feature = "audio")]
use hawkwing::query::Query;
#[cfg(feature = "audio")]
use hawkwing::resolver::local::LocalResolver;
#[cfg(feature = "audio")]
use hawkwing::resolver::{load_resolvers, Orchestrator, Resolver};

#[derive(Parser)]
#[command(name = "hawkwing", about = "Music player that decouples metadata from sources")]
struct Cli {
    /// Path to the library database (default: ~/.local/share/hawkwing/library.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Directory containing resolver executables (default: ~/.local/share/hawkwing/resolvers/)
    #[arg(long, global = true)]
    resolvers: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Walk a directory and index all audio files
    Scan { path: PathBuf },
    /// Full-text search across title, artist, and album
    Search { query: String },
    /// List all indexed tracks
    List,
    /// Resolve and play a track
    Play { artist: String, title: String },
}

#[cfg(feature = "audio")]
fn build_orchestrator(db: Arc<Database>, resolver_dir: &std::path::Path) -> Orchestrator {
    let mut resolvers: Vec<Box<dyn Resolver>> = vec![
        Box::new(LocalResolver::new(Arc::clone(&db))),
    ];
    resolvers.extend(load_resolvers(resolver_dir));
    Orchestrator::new(resolvers)
}

#[cfg(feature = "audio")]
fn cmd_play(db: Arc<Database>, resolver_dir: &std::path::Path, artist: String, title: String) -> Result<()> {
    use hawkwing::query::Source;

    let query = Query::new(&artist, &title);
    let orchestrator = build_orchestrator(db, resolver_dir);
    let results = orchestrator.resolve(&query)?;
    let Some(best) = results.into_iter().next() else {
        bail!("No results found for \"{artist} — {title}\"");
    };

    match &best.source {
        Source::LocalFile(filepath) => {
            println!("Playing: {} — {} ({})", best.artist, best.title, filepath);
            let player = Player::new()?;
            player.play_file(std::path::Path::new(filepath))?;
            player.sleep_until_end();
        }
        Source::Url { url, .. } => {
            println!("Playing: {} — {} ({})", best.artist, best.title, url);
            bail!("URL streaming not yet implemented");
        }
    }
    Ok(())
}

#[cfg(not(feature = "audio"))]
fn cmd_play(_db: Arc<Database>, _resolver_dir: &std::path::Path, _artist: String, _title: String) -> Result<()> {
    bail!("Audio support not compiled in. Rebuild with `--features audio` (requires libasound2-dev).")
}

fn default_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".local/share/hawkwing/library.db"))
}

fn default_resolver_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".local/share/hawkwing/resolvers"))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let db_path = match cli.db {
        Some(p) => p,
        None => default_db_path()?,
    };
    let resolver_dir = match cli.resolvers {
        Some(p) => p,
        None => default_resolver_dir()?,
    };

    let db = Arc::new(Database::open(&db_path)?);

    match cli.command {
        Command::Scan { path } => {
            println!("Scanning {}…", path.display());
            let n = scan_directory(&db, &path)?;
            println!("Indexed {n} track(s).");
        }

        Command::Search { query } => {
            let tracks = db.search_tracks(&query)?;
            if tracks.is_empty() {
                println!("No results.");
            } else {
                for t in &tracks {
                    println!("{} — {} [{}]", t.artist, t.title, t.album);
                }
            }
        }

        Command::List => {
            let tracks = db.list_tracks()?;
            if tracks.is_empty() {
                println!("Library is empty. Run `hawkwing scan <path>` first.");
            } else {
                for t in &tracks {
                    println!("{} — {} [{}]", t.artist, t.title, t.album);
                }
            }
        }

        Command::Play { artist, title } => {
            cmd_play(Arc::clone(&db), &resolver_dir, artist, title)?;
        }
    }

    Ok(())
}
