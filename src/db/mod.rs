use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;

const SCHEMA: &str = include_str!("schema.sql");

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct TrackRow {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub filepath: String,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create db directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open database {}", path.display()))?;
        let db = Database { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("open in-memory database")?;
        let db = Database { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute_batch(SCHEMA)
            .context("apply schema migrations")?;
        Ok(())
    }

    pub fn upsert_track(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        duration_ms: i64,
        filepath: &str,
    ) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO track (title, artist, album, duration_ms, filepath)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(filepath) DO UPDATE SET
               title       = excluded.title,
               artist      = excluded.artist,
               album       = excluded.album,
               duration_ms = excluded.duration_ms",
            params![title, artist, album, duration_ms, filepath],
        )?;
        Ok(())
    }

    pub fn find_track(&self, artist: &str, title: &str) -> Result<Option<TrackRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, artist, album, duration_ms, filepath
             FROM track
             WHERE lower(artist) = lower(?1) AND lower(title) = lower(?2)
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![artist, title])?;
        if let Some(row) = rows.next()? {
            Ok(Some(TrackRow {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                duration_ms: row.get(4)?,
                filepath: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn search_tracks(&self, query: &str) -> Result<Vec<TrackRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.title, t.artist, t.album, t.duration_ms, t.filepath
             FROM track_fts
             JOIN track t ON track_fts.rowid = t.id
             WHERE track_fts MATCH ?1
             ORDER BY rank",
        )?;
        let rows = stmt.query_map(params![query], |row| {
            Ok(TrackRow {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                duration_ms: row.get(4)?,
                filepath: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn list_tracks(&self) -> Result<Vec<TrackRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, artist, album, duration_ms, filepath
             FROM track
             ORDER BY artist, album, title",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TrackRow {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                duration_ms: row.get(4)?,
                filepath: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn get_track_by_id(&self, id: i64) -> Result<Option<TrackRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, artist, album, duration_ms, filepath
             FROM track WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(TrackRow {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                duration_ms: row.get(4)?,
                filepath: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn track_count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM track", [], |r| r.get(0))?;
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_upsert_and_find() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_track("Bohemian Rhapsody", "Queen", "A Night at the Opera", 354000, "/music/br.mp3").unwrap();
        let hit = db.find_track("queen", "bohemian rhapsody").unwrap();
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().artist, "Queen");
    }

    #[test]
    fn upsert_updates_existing() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_track("Song", "Artist", "Album", 1000, "/f.mp3").unwrap();
        db.upsert_track("Song v2", "Artist", "Album 2", 2000, "/f.mp3").unwrap();
        assert_eq!(db.track_count().unwrap(), 1);
        let row = db.find_track("Artist", "Song v2").unwrap();
        assert!(row.is_some());
    }

    #[test]
    fn find_miss_returns_none() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.find_track("nobody", "nothing").unwrap().is_none());
    }

    #[test]
    fn fts_search() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_track("Dark Side", "Pink Floyd", "DSOTM", 240000, "/ds.flac").unwrap();
        db.upsert_track("Comfortably Numb", "Pink Floyd", "The Wall", 382000, "/cn.flac").unwrap();
        let results = db.search_tracks("Numb").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Comfortably Numb");
    }

    #[test]
    fn get_track_by_id_returns_correct_row() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_track("Heroes", "David Bowie", "Heroes", 369000, "/heroes.flac").unwrap();
        let row = db.find_track("David Bowie", "Heroes").unwrap().unwrap();
        let found = db.get_track_by_id(row.id).unwrap().unwrap();
        assert_eq!(found.title, "Heroes");
        assert_eq!(found.id, row.id);
    }

    #[test]
    fn get_track_by_id_miss_returns_none() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.get_track_by_id(9999).unwrap().is_none());
    }
}
