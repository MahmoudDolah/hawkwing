use std::sync::Arc;

use hawkwing::db::Database;
use hawkwing::query::Query;
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
