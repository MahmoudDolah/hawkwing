use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use hawkwing::db::Database;
use hawkwing::scanner::scan_directory;

#[cfg(feature = "audio")]
use hawkwing::resolver::local::LocalResolver;
#[cfg(feature = "audio")]
use hawkwing::resolver::{load_resolvers, Orchestrator, Resolver};

#[cfg(feature = "audio")]
use hawkwing::audio::Player;
#[cfg(feature = "audio")]
use hawkwing::query::Query;

#[derive(Parser)]
#[command(name = "hawkwing", about = "Music player that decouples metadata from sources")]
struct Cli {
    /// Path to the library database (default: ~/.local/share/hawkwing/library.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Directory containing resolver executables (default: ~/.local/share/hawkwing/resolvers/)
    #[arg(long, global = true)]
    resolvers: Option<PathBuf>,

    /// Enable P2P discovery via mDNS
    #[arg(long, global = true)]
    p2p: bool,

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

fn data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".local/share/hawkwing"))
}

fn default_db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("library.db"))
}

fn default_resolver_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("resolvers"))
}

#[cfg(feature = "audio")]
fn cmd_play(
    db: Arc<Database>,
    resolver_dir: &std::path::Path,
    p2p: bool,
    artist: String,
    title: String,
) -> Result<()> {
    use hawkwing::query::Source;

    let mut resolvers: Vec<Box<dyn Resolver>> = vec![Box::new(LocalResolver::new(Arc::clone(&db)))];
    resolvers.extend(load_resolvers(resolver_dir));

    // Start P2P node and discover peers if requested.
    // _node must stay alive for the duration of playback (keeps mDNS announced + server running).
    let _node;
    if p2p {
        let dir = data_dir()?;
        let node_id = hawkwing::node_id::get_or_create(&dir)?;
        let node = hawkwing::p2p::Node::start(Arc::clone(&db), node_id)?;
        let peers = node.discover_peers()?;
        for peer in peers {
            resolvers.push(Box::new(peer));
        }
        _node = Some(node);
    } else {
        _node = None;
    }

    let orchestrator = Orchestrator::new(resolvers);
    let query = Query::new(&artist, &title);
    let results = orchestrator.resolve(&query)?;
    let Some(best) = results.into_iter().next() else {
        bail!("No results found for \"{artist} — {title}\"");
    };

    match &best.source {
        Source::LocalFile(filepath) => {
            println!("Playing: {} — {} (local: {})", best.artist, best.title, filepath);
            let player = Player::new()?;
            player.play_file(std::path::Path::new(filepath))?;
            player.sleep_until_end();
        }
        Source::Url { url, .. } => {
            println!("Playing: {} — {} (peer: {})", best.artist, best.title, url);
            println!("Downloading from peer…");
            let mut tmp = tempfile::NamedTempFile::new().context("create temp file")?;
            let resp = ureq::get(url).call().context("download from peer")?;
            std::io::copy(&mut resp.into_reader(), &mut tmp).context("write temp file")?;
            let player = Player::new()?;
            player.play_file(tmp.path())?;
            player.sleep_until_end();
        }
    }
    Ok(())
}

#[cfg(not(feature = "audio"))]
fn cmd_play(
    _db: Arc<Database>,
    _resolver_dir: &std::path::Path,
    _p2p: bool,
    _artist: String,
    _title: String,
) -> Result<()> {
    bail!("Audio support not compiled in. Rebuild with `--features audio` (requires libasound2-dev).")
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let db_path = cli.db.unwrap_or(default_db_path()?);
    let resolver_dir = cli.resolvers.unwrap_or(default_resolver_dir()?);

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
            cmd_play(Arc::clone(&db), &resolver_dir, cli.p2p, artist, title)?;
        }
    }

    Ok(())
}
