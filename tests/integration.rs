use std::io::Write;
use std::sync::Arc;

use hawkwing::db::Database;
use hawkwing::p2p::server::PeerServer;
use hawkwing::query::{Query, Source};
use hawkwing::resolver::http::HttpResolver;
use hawkwing::resolver::local::LocalResolver;
use hawkwing::resolver::{Orchestrator, Resolver};

fn seeded_db() -> Arc<Database> {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.upsert_track("Wish You Were Here", "Pink Floyd", "Wish You Were Here", 334000, "/music/wywh.flac").unwrap();
    db.upsert_track("Money", "Pink Floyd", "The Dark Side of the Moon", 382000, "/music/money.flac").unwrap();
    db.upsert_track("Stairway to Heaven", "Led Zeppelin", "Led Zeppelin IV", 482000, "/music/stairway.mp3").unwrap();
    db
}

#[test]
fn orchestrator_finds_exact_match() {
    let db = seeded_db();
    let orc = Orchestrator::new(vec![
        Box::new(LocalResolver::new(Arc::clone(&db))) as Box<dyn Resolver>,
    ]);
    let results = orc.resolve(&Query::new("Pink Floyd", "Money")).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].score, 1.0);
    assert_eq!(results[0].title, "Money");
}

#[test]
fn orchestrator_case_insensitive() {
    let db = seeded_db();
    let orc = Orchestrator::new(vec![
        Box::new(LocalResolver::new(Arc::clone(&db))) as Box<dyn Resolver>,
    ]);
    let results = orc.resolve(&Query::new("led zeppelin", "stairway to heaven")).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn orchestrator_returns_empty_on_miss() {
    let db = seeded_db();
    let orc = Orchestrator::new(vec![
        Box::new(LocalResolver::new(Arc::clone(&db))) as Box<dyn Resolver>,
    ]);
    let results = orc.resolve(&Query::new("Beatles", "Yesterday")).unwrap();
    assert!(results.is_empty());
}

#[test]
fn db_track_count_after_inserts() {
    let db = seeded_db();
    assert_eq!(db.track_count().unwrap(), 3);
}

#[test]
fn db_fts_search_returns_partial_match() {
    let db = seeded_db();
    let results = db.search_tracks("Floyd").unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn db_list_is_sorted() {
    let db = seeded_db();
    let tracks = db.list_tracks().unwrap();
    // Artist-sorted: Led Zeppelin before Pink Floyd
    assert_eq!(tracks[0].artist, "Led Zeppelin");
    assert_eq!(tracks[1].artist, "Pink Floyd");
}

// ── Phase 2 + 3: PeerServer + HttpResolver end-to-end ────────────────────────

/// Start a PeerServer backed by a seeded in-memory DB, discover it via
/// HttpResolver::connect_peer, and resolve through an Orchestrator. Verifies
/// the full path from HTTP discovery → resolve → relative URL expansion.
#[test]
fn peer_server_resolves_via_http_resolver() {
    let peer_db = Arc::new(Database::open_in_memory().unwrap());
    peer_db
        .upsert_track("Heroes", "David Bowie", "Heroes", 369000, "/heroes.flac")
        .unwrap();

    let server = PeerServer::start(Arc::clone(&peer_db)).unwrap();
    let base_url = format!("http://127.0.0.1:{}", server.port);

    let peer_resolver = HttpResolver::connect_peer(base_url.clone()).unwrap();

    let orc = Orchestrator::new(vec![Box::new(peer_resolver) as Box<dyn Resolver>]);
    let results = orc.resolve(&Query::new("David Bowie", "Heroes")).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Heroes");
    assert_eq!(results[0].score, 1.0);
    // URL must be fully formed (base_url + /track/<id>), not a bare relative path
    assert!(
        matches!(&results[0].source, Source::Url { url, .. }
            if url.starts_with(&base_url) && url.contains("/track/")),
        "expected absolute URL starting with {base_url}, got {:?}",
        results[0].source
    );
}

/// LocalResolver (weight 100) beats PeerServer (weight 80): local result wins.
#[test]
fn local_resolver_outranks_peer() {
    let local_db = seeded_db();

    let peer_db = Arc::new(Database::open_in_memory().unwrap());
    peer_db
        .upsert_track("Money", "Pink Floyd", "DSOTM", 382000, "/peer/money.flac")
        .unwrap();
    let server = PeerServer::start(Arc::clone(&peer_db)).unwrap();
    let peer_resolver =
        HttpResolver::connect_peer(format!("http://127.0.0.1:{}", server.port)).unwrap();

    let orc = Orchestrator::new(vec![
        Box::new(LocalResolver::new(Arc::clone(&local_db))) as Box<dyn Resolver>,
        Box::new(peer_resolver),
    ]);

    let results = orc.resolve(&Query::new("Pink Floyd", "Money")).unwrap();
    // Both resolvers return score 1.0; Orchestrator's sort is stable so the
    // first-added (local) result comes first when scores are equal.
    assert!(matches!(&results[0].source, Source::LocalFile(_)));
}

/// PeerServer returns a track only the peer has; local DB misses.
#[test]
fn peer_only_track_resolved_from_peer() {
    let local_db = Arc::new(Database::open_in_memory().unwrap()); // empty

    let peer_db = Arc::new(Database::open_in_memory().unwrap());
    peer_db
        .upsert_track("Peer-Only Song", "Remote Artist", "", 120000, "/remote.mp3")
        .unwrap();
    let server = PeerServer::start(Arc::clone(&peer_db)).unwrap();
    let peer_resolver =
        HttpResolver::connect_peer(format!("http://127.0.0.1:{}", server.port)).unwrap();

    let orc = Orchestrator::new(vec![
        Box::new(LocalResolver::new(local_db)) as Box<dyn Resolver>,
        Box::new(peer_resolver),
    ]);

    let results = orc
        .resolve(&Query::new("Remote Artist", "Peer-Only Song"))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(matches!(&results[0].source, Source::Url { .. }));
}

/// PeerServer serves correct bytes for GET /track/<id>.
#[test]
fn peer_server_track_bytes_match_file() {
    let content = b"this is fake audio data";
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(content).unwrap();
    tmp.flush().unwrap();

    let db = Arc::new(Database::open_in_memory().unwrap());
    db.upsert_track("Fake", "Test", "", 0, tmp.path().to_str().unwrap())
        .unwrap();

    let server = PeerServer::start(Arc::clone(&db)).unwrap();
    let row = db.find_track("Test", "Fake").unwrap().unwrap();

    let mut body = Vec::new();
    ureq::get(&format!("http://127.0.0.1:{}/track/{}", server.port, row.id))
        .call()
        .unwrap()
        .into_reader()
        .read_to_end(&mut body)
        .unwrap();

    assert_eq!(body, content);
}
