/// What the user wants to play.
#[derive(Debug, Clone)]
pub struct Query {
    pub artist: String,
    pub title: String,
}

impl Query {
    pub fn new(artist: impl Into<String>, title: impl Into<String>) -> Self {
        Query {
            artist: artist.into(),
            title: title.into(),
        }
    }
}

/// Where a track can be obtained from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    LocalFile(String),
    Url { url: String, mimetype: String },
}

/// A resolver's answer for a given Query.
#[derive(Debug, Clone)]
pub struct ResolveResult {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration_ms: i64,
    pub source: Source,
    /// Confidence in this result; 0.0–1.0. Higher is preferred.
    pub score: f32,
}
