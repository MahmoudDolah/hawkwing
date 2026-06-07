use anyhow::{Context, Result};
use std::path::Path;
use uuid::Uuid;

/// Returns the persistent node ID, creating and saving one if it doesn't exist yet.
pub fn get_or_create(data_dir: &Path) -> Result<String> {
    let path = data_dir.join("node_id");
    if path.exists() {
        let id = std::fs::read_to_string(&path).context("read node_id")?;
        let id = id.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }
    std::fs::create_dir_all(data_dir).context("create data directory")?;
    let id = Uuid::new_v4().to_string();
    std::fs::write(&path, &id).context("write node_id")?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_uuid_on_first_call() {
        let dir = tempfile::tempdir().unwrap();
        let id = get_or_create(dir.path()).unwrap();
        assert!(!id.is_empty());
        // Valid UUID format: 8-4-4-4-12 hex chars
        assert_eq!(id.len(), 36);
    }

    #[test]
    fn returns_same_id_on_subsequent_calls() {
        let dir = tempfile::tempdir().unwrap();
        let id1 = get_or_create(dir.path()).unwrap();
        let id2 = get_or_create(dir.path()).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn persists_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let id = get_or_create(dir.path()).unwrap();
        let on_disk = std::fs::read_to_string(dir.path().join("node_id")).unwrap();
        assert_eq!(on_disk.trim(), id);
    }
}
