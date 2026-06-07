use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

use super::process::ResolverProcess;
use super::Resolver;
use crate::query::{Query, ResolveResult, Source};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ResolveRequest<'a> {
    artist: &'a str,
    title: &'a str,
    album: &'a str,
}

#[derive(Deserialize)]
struct ResolveResponse {
    results: Vec<WireResult>,
}

#[derive(Deserialize)]
struct WireResult {
    artist: String,
    title: String,
    #[serde(default)]
    album: String,
    url: String,
    #[serde(default)]
    mimetype: String,
    /// Duration in milliseconds
    #[serde(default)]
    duration_ms: i64,
    #[serde(default)]
    score: f32,
}

#[derive(Deserialize)]
struct InfoResponse {
    name: String,
    #[serde(default = "default_weight")]
    weight: u8,
}

fn default_weight() -> u8 {
    50
}

// ── HttpResolver ──────────────────────────────────────────────────────────────

pub struct HttpResolver {
    _process: Option<ResolverProcess>,
    name: String,
    weight: u8,
    base_url: String,
    agent: ureq::Agent,
}

impl HttpResolver {
    /// Connect to an already-spawned resolver process, fetching /info for metadata.
    pub fn connect(process: ResolverProcess) -> Result<Self> {
        let base_url = format!("http://127.0.0.1:{}", process.port);
        let agent = ureq::AgentBuilder::new()
            .timeout(REQUEST_TIMEOUT)
            .build();

        let info: InfoResponse = agent
            .get(&format!("{base_url}/info"))
            .call()
            .context("GET /info from resolver")?
            .into_json()
            .context("parse /info response")?;

        Ok(HttpResolver {
            _process: Some(process),
            name: info.name,
            weight: info.weight,
            base_url,
            agent,
        })
    }

    /// For testing: connect directly to a known URL without a subprocess.
    #[cfg(test)]
    pub fn connect_url(base_url: String, name: String, weight: u8) -> Self {
        HttpResolver {
            _process: None,
            name,
            weight,
            base_url,
            agent: ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build(),
        }
    }

    /// Spawn the resolver at `path` and connect to it.
    pub fn spawn(path: &Path) -> Result<Self> {
        let process = ResolverProcess::spawn(path)?;
        Self::connect(process)
    }

    /// Connect to a peer's HTTP server by URL (no subprocess).
    pub fn connect_peer(base_url: String) -> Result<Self> {
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        let info: InfoResponse = agent
            .get(&format!("{base_url}/info"))
            .call()
            .context("GET /info from peer")?
            .into_json()
            .context("parse /info response")?;
        Ok(HttpResolver {
            _process: None,
            name: info.name,
            weight: info.weight,
            base_url,
            agent,
        })
    }
}

impl Resolver for HttpResolver {
    fn name(&self) -> &str {
        &self.name
    }

    fn weight(&self) -> u8 {
        self.weight
    }

    fn resolve(&self, query: &Query) -> Result<Vec<ResolveResult>> {
        let req = ResolveRequest {
            artist: &query.artist,
            title: &query.title,
            album: "",
        };

        let resp: ResolveResponse = self
            .agent
            .post(&format!("{}/resolve", self.base_url))
            .send_json(serde_json::to_value(&req)?)
            .context("POST /resolve")?
            .into_json()
            .context("parse /resolve response")?;

        let results = resp
            .results
            .into_iter()
            .filter(|r| !r.artist.is_empty() && !r.title.is_empty() && !r.url.is_empty())
            .map(|r| {
                // Peer servers return relative URLs like /track/<id>; prepend base_url.
                let url = if r.url.starts_with('/') {
                    format!("{}{}", self.base_url, r.url)
                } else {
                    r.url
                };
                ResolveResult {
                artist: r.artist,
                title: r.title,
                album: r.album,
                duration_ms: r.duration_ms,
                source: Source::Url {
                    url,
                    mimetype: r.mimetype,
                },
                score: r.score,
            }
            })
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    /// Spawn a minimal HTTP/1.1 server in a thread. Returns the port it bound to.
    /// `handler` receives the raw request path + body and returns a JSON string to send.
    fn mock_server(responses: Vec<(&'static str, &'static str)>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let responses: Vec<(String, String)> = responses
            .into_iter()
            .map(|(p, r)| (p.to_string(), r.to_string()))
            .collect();

        thread::spawn(move || {
            for (expected_path, response_body) in &responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let req = std::str::from_utf8(&buf[..n]).unwrap_or("");
                let path = req.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("");
                assert!(
                    path.starts_with(expected_path.as_str()),
                    "expected path {expected_path}, got {path}"
                );
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream.write_all(resp.as_bytes()).unwrap();
            }
        });

        port
    }

    use std::io::Read;

    #[test]
    fn resolve_returns_results() {
        let resolve_body = r#"{
            "results": [{
                "artist": "Pink Floyd",
                "title": "Money",
                "album": "DSOTM",
                "url": "https://example.com/money.mp3",
                "mimetype": "audio/mpeg",
                "duration_ms": 382000,
                "score": 0.9
            }]
        }"#;

        let port = mock_server(vec![("/resolve", resolve_body)]);
        let resolver = HttpResolver::connect_url(
            format!("http://127.0.0.1:{port}"),
            "TestResolver".into(),
            60,
        );

        let results = resolver.resolve(&Query::new("Pink Floyd", "Money")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Money");
        assert_eq!(results[0].score, 0.9);
        assert!(matches!(&results[0].source, Source::Url { url, .. } if url.contains("money")));
    }

    #[test]
    fn filters_out_incomplete_results() {
        let port = mock_server(vec![("/resolve", r#"{"results":[{"artist":"","title":"","url":"x"}]}"#)]);
        let resolver = HttpResolver::connect_url(
            format!("http://127.0.0.1:{port}"),
            "TestResolver".into(),
            50,
        );
        let results = resolver.resolve(&Query::new("x", "y")).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn relative_url_is_expanded_with_base_url() {
        let body = r#"{"results":[{
            "artist":"A","title":"T","album":"",
            "url":"/track/42","duration_ms":1000,"score":1.0
        }]}"#;
        let port = mock_server(vec![("/resolve", body)]);
        let base = format!("http://127.0.0.1:{port}");
        let resolver = HttpResolver::connect_url(base.clone(), "Peer".into(), 80);
        let results = resolver.resolve(&Query::new("A", "T")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0].source, Source::Url { url, .. } if *url == format!("{base}/track/42")),
            "expected base_url prepended to /track/42"
        );
    }

    #[test]
    fn absolute_url_is_left_unchanged() {
        let body = r#"{"results":[{
            "artist":"A","title":"T","album":"",
            "url":"https://cdn.example.com/song.mp3","duration_ms":1000,"score":0.8
        }]}"#;
        let port = mock_server(vec![("/resolve", body)]);
        let resolver = HttpResolver::connect_url(
            format!("http://127.0.0.1:{port}"),
            "Peer".into(),
            80,
        );
        let results = resolver.resolve(&Query::new("A", "T")).unwrap();
        assert!(
            matches!(&results[0].source, Source::Url { url, .. } if url == "https://cdn.example.com/song.mp3")
        );
    }
}
