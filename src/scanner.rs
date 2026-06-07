use anyhow::Result;
use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::db::Database;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "wav", "m4a", "aac", "opus", "oga"];

pub fn scan_directory(db: &Database, root: &Path) -> Result<usize> {
    let mut indexed = 0usize;

    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        let Some(ext) = ext else { continue };
        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        match index_file(db, path) {
            Ok(()) => {
                indexed += 1;
                info!(path = %path.display(), "indexed");
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "skipping file");
            }
        }
    }

    Ok(indexed)
}

fn index_file(db: &Database, path: &Path) -> Result<()> {
    let tagged = Probe::open(path)?.guess_file_type()?.read()?;

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let title = tag
        .and_then(|t| t.title().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });

    let artist = tag
        .and_then(|t| t.artist().map(|s| s.to_string()))
        .unwrap_or_default();

    let album = tag
        .and_then(|t| t.album().map(|s| s.to_string()))
        .unwrap_or_default();

    let duration_ms = tagged
        .properties()
        .duration()
        .as_millis()
        .try_into()
        .unwrap_or(0i64);

    let filepath = path
        .canonicalize()?
        .to_string_lossy()
        .into_owned();

    db.upsert_track(&title, &artist, &album, duration_ms, &filepath)?;
    Ok(())
}
