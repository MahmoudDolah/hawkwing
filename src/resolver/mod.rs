pub mod http;
pub mod local;
pub mod process;

use anyhow::Result;
use std::path::Path;
use crate::query::{Query, ResolveResult};

pub trait Resolver: Send + Sync {
    fn name(&self) -> &str;
    fn weight(&self) -> u8;
    fn resolve(&self, query: &Query) -> Result<Vec<ResolveResult>>;
}

pub struct Orchestrator {
    resolvers: Vec<Box<dyn Resolver>>,
}

impl Orchestrator {
    pub fn new(resolvers: Vec<Box<dyn Resolver>>) -> Self {
        let mut r = resolvers;
        r.sort_by(|a, b| b.weight().cmp(&a.weight()));
        Orchestrator { resolvers: r }
    }

    pub fn resolve(&self, query: &Query) -> Result<Vec<ResolveResult>> {
        let mut all: Vec<ResolveResult> = Vec::new();
        for resolver in &self.resolvers {
            match resolver.resolve(query) {
                Ok(mut results) => all.append(&mut results),
                Err(e) => tracing::warn!(resolver = resolver.name(), error = %e, "resolver failed"),
            }
        }
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(all)
    }
}

/// Load all executable files in `dir` as HTTP resolvers.
/// Non-executables and files that fail to spawn are skipped with a warning.
pub fn load_resolvers(dir: &Path) -> Vec<Box<dyn Resolver>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };

    let mut resolvers: Vec<Box<dyn Resolver>> = Vec::new();

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !is_executable(&path) {
            continue;
        }
        match http::HttpResolver::spawn(&path) {
            Ok(r) => {
                tracing::info!(name = r.name(), weight = r.weight(), "loaded resolver");
                resolvers.push(Box::new(r));
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to load resolver");
            }
        }
    }

    resolvers
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}
