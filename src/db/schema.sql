CREATE TABLE IF NOT EXISTS track (
    id          INTEGER PRIMARY KEY,
    title       TEXT NOT NULL,
    artist      TEXT NOT NULL DEFAULT '',
    album       TEXT NOT NULL DEFAULT '',
    duration_ms INTEGER NOT NULL DEFAULT 0,
    filepath    TEXT NOT NULL UNIQUE,
    added_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE VIRTUAL TABLE IF NOT EXISTS track_fts USING fts5(
    title,
    artist,
    album,
    content=track,
    content_rowid=id
);

CREATE TRIGGER IF NOT EXISTS track_ai AFTER INSERT ON track BEGIN
    INSERT INTO track_fts(rowid, title, artist, album)
    VALUES (new.id, new.title, new.artist, new.album);
END;

CREATE TRIGGER IF NOT EXISTS track_ad AFTER DELETE ON track BEGIN
    INSERT INTO track_fts(track_fts, rowid, title, artist, album)
    VALUES ('delete', old.id, old.title, old.artist, old.album);
END;

CREATE TRIGGER IF NOT EXISTS track_au AFTER UPDATE ON track BEGIN
    INSERT INTO track_fts(track_fts, rowid, title, artist, album)
    VALUES ('delete', old.id, old.title, old.artist, old.album);
    INSERT INTO track_fts(rowid, title, artist, album)
    VALUES (new.id, new.title, new.artist, new.album);
END;
