use anyhow::Result;
use std::sync::Arc;

use crate::db::Database;
use crate::query::{Query, ResolveResult, Source};
use super::Resolver;

pub struct LocalResolver {
    db: Arc<Database>,
}

impl LocalResolver {
    pub fn new(db: Arc<Database>) -> Self {
        LocalResolver { db }
    }
}

impl Resolver for LocalResolver {
    fn name(&self) -> &str {
        "local"
    }

    fn weight(&self) -> u8 {
        100
    }

    fn resolve(&self, query: &Query) -> Result<Vec<ResolveResult>> {
        match self.db.find_track(&query.artist, &query.title)? {
            None => Ok(vec![]),
            Some(row) => Ok(vec![ResolveResult {
                artist: row.artist,
                title: row.title,
                album: row.album,
                duration_ms: row.duration_ms,
                source: Source::LocalFile(row.filepath),
                score: 1.0,
            }]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::query::Query;

    fn make_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn resolves_known_track() {
        let db = make_db();
        db.upsert_track("Heroes", "David Bowie", "Heroes", 369000, "/music/heroes.flac").unwrap();
        let resolver = LocalResolver::new(db);
        let results = resolver.resolve(&Query::new("David Bowie", "Heroes")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 1.0);
        assert!(matches!(&results[0].source, Source::LocalFile(p) if p == "/music/heroes.flac"));
    }

    #[test]
    fn resolves_case_insensitive() {
        let db = make_db();
        db.upsert_track("Space Oddity", "David Bowie", "", 315000, "/music/so.mp3").unwrap();
        let resolver = LocalResolver::new(db);
        let results = resolver.resolve(&Query::new("david bowie", "space oddity")).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn returns_empty_on_miss() {
        let db = make_db();
        let resolver = LocalResolver::new(db);
        let results = resolver.resolve(&Query::new("Nobody", "Nothing")).unwrap();
        assert!(results.is_empty());
    }
}
