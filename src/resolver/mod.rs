pub mod local;

use anyhow::Result;
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
