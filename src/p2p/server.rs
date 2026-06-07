use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::db::Database;

const PEER_WEIGHT: u8 = 80;

pub struct PeerServer {
    pub port: u16,
    _thread: JoinHandle<()>,
}

impl PeerServer {
    pub fn start(db: Arc<Database>) -> Result<Self> {
        let server =
            Server::http("0.0.0.0:0").map_err(|e| anyhow::anyhow!("bind peer server: {e}"))?;
        let port = server
            .server_addr()
            .to_ip()
            .context("get server port")?
            .port();

        let thread = thread::spawn(move || run(server, db));

        Ok(PeerServer {
            port,
            _thread: thread,
        })
    }
}

fn run(server: Server, db: Arc<Database>) {
    for request in server.incoming_requests() {
        let db = Arc::clone(&db);
        thread::spawn(move || {
            if let Err(e) = handle(request, &db) {
                tracing::warn!(error = %e, "peer server: request error");
            }
        });
    }
}

fn json_header() -> Header {
    Header::from_bytes("Content-Type", "application/json").unwrap()
}

fn handle(request: tiny_http::Request, db: &Database) -> Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();

    match (&method, url.as_str()) {
        (Method::Get, "/info") => handle_info(request),
        (Method::Post, "/resolve") => handle_resolve(request, db),
        (Method::Get, path) if path.starts_with("/track/") => handle_track(request, db, path),
        _ => {
            request
                .respond(Response::from_string("not found").with_status_code(404))
                .ok();
            Ok(())
        }
    }
}

// GET /info
fn handle_info(request: tiny_http::Request) -> Result<()> {
    #[derive(Serialize)]
    struct Info {
        name: String,
        weight: u8,
    }
    let name = hostname();
    let body = serde_json::to_string(&Info { name, weight: PEER_WEIGHT })?;
    request
        .respond(Response::from_string(body).with_header(json_header()))
        .context("send /info response")
}

// POST /resolve
fn handle_resolve(mut request: tiny_http::Request, db: &Database) -> Result<()> {
    #[derive(Deserialize)]
    struct Req {
        artist: String,
        title: String,
    }
    #[derive(Serialize)]
    struct Resp {
        results: Vec<WireResult>,
    }
    #[derive(Serialize)]
    struct WireResult {
        artist: String,
        title: String,
        album: String,
        duration_ms: i64,
        url: String,
        score: f32,
    }

    let req: Req = serde_json::from_reader(request.as_reader())
        .context("parse resolve request body")?;

    let results = match db.find_track(&req.artist, &req.title)? {
        None => vec![],
        Some(row) => vec![WireResult {
            artist: row.artist,
            title: row.title,
            album: row.album,
            duration_ms: row.duration_ms,
            url: format!("/track/{}", row.id),
            score: 1.0,
        }],
    };

    let body = serde_json::to_string(&Resp { results })?;
    request
        .respond(Response::from_string(body).with_header(json_header()))
        .context("send /resolve response")
}

// GET /track/<id>
fn handle_track(request: tiny_http::Request, db: &Database, path: &str) -> Result<()> {
    let id_str = path.trim_start_matches("/track/");
    let id: i64 = id_str
        .parse()
        .with_context(|| format!("invalid track id: {id_str}"))?;

    let track = db
        .get_track_by_id(id)?
        .ok_or_else(|| anyhow::anyhow!("track {id} not found"))?;

    let mut file =
        std::fs::File::open(&track.filepath).context("open track file")?;
    let total = file.metadata().context("stat track file")?.len();

    // Parse optional Range header
    let range = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .map(|h| h.value.as_str().to_string());

    let accept_ranges = Header::from_bytes("Accept-Ranges", "bytes").unwrap();

    if let Some(range_val) = range {
        if let Some((start, end)) = parse_range(&range_val, total) {
            let len = end - start + 1;
            file.seek(SeekFrom::Start(start)).context("seek in track file")?;
            let reader = file.take(len);
            let response = Response::new(
                StatusCode(206),
                vec![
                    accept_ranges,
                    Header::from_bytes(
                        "Content-Range",
                        format!("bytes {start}-{end}/{total}"),
                    )
                    .unwrap(),
                    Header::from_bytes("Content-Type", "audio/octet-stream").unwrap(),
                ],
                reader,
                Some(len as usize),
                None,
            );
            request.respond(response).context("send range response")?;
            return Ok(());
        }
    }

    // Full file
    let response = Response::new(
        StatusCode(200),
        vec![
            accept_ranges,
            Header::from_bytes("Content-Type", "audio/octet-stream").unwrap(),
        ],
        file,
        Some(total as usize),
        None,
    );
    request.respond(response).context("send track response")
}

fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let s = header.strip_prefix("bytes=")?;
    let (start_s, end_s) = s.split_once('-')?;
    let start: u64 = start_s.parse().ok()?;
    let end: u64 = if end_s.is_empty() {
        total.saturating_sub(1)
    } else {
        end_s.parse().ok()?
    };
    if start > end || end >= total {
        return None;
    }
    Some((start, end))
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "hawkwing".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::io::Write;
    use std::sync::Arc;

    fn test_server() -> (PeerServer, Arc<Database>) {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let server = PeerServer::start(Arc::clone(&db)).unwrap();
        (server, db)
    }

    fn base_url(server: &PeerServer) -> String {
        format!("http://127.0.0.1:{}", server.port)
    }

    // ── Range parsing ─────────────────────────────────────────────────────────

    #[test]
    fn range_full() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
    }

    #[test]
    fn range_open_end() {
        assert_eq!(parse_range("bytes=500-", 1000), Some((500, 999)));
    }

    #[test]
    fn range_invalid_beyond_file() {
        assert_eq!(parse_range("bytes=0-9999", 100), None);
    }

    #[test]
    fn range_start_after_end() {
        assert_eq!(parse_range("bytes=50-10", 100), None);
    }

    // ── HTTP endpoints ────────────────────────────────────────────────────────

    #[test]
    fn info_endpoint_returns_name_and_weight() {
        let (server, _db) = test_server();
        let resp: serde_json::Value = ureq::get(&format!("{}/info", base_url(&server)))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert!(resp["name"].is_string());
        assert_eq!(resp["weight"].as_u64().unwrap(), PEER_WEIGHT as u64);
    }

    #[test]
    fn resolve_endpoint_hit() {
        let (server, db) = test_server();
        db.upsert_track("Space Oddity", "David Bowie", "", 315000, "/space.mp3")
            .unwrap();

        let body = serde_json::json!({"artist": "David Bowie", "title": "Space Oddity"});
        let resp: serde_json::Value = ureq::post(&format!("{}/resolve", base_url(&server)))
            .send_json(body)
            .unwrap()
            .into_json()
            .unwrap();

        let results = resp["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "Space Oddity");
        // URL should be a relative /track/<id>
        assert!(results[0]["url"].as_str().unwrap().starts_with("/track/"));
        assert_eq!(results[0]["score"].as_f64().unwrap(), 1.0);
    }

    #[test]
    fn resolve_endpoint_miss_returns_empty() {
        let (server, _db) = test_server();
        let body = serde_json::json!({"artist": "Nobody", "title": "Nothing"});
        let resp: serde_json::Value = ureq::post(&format!("{}/resolve", base_url(&server)))
            .send_json(body)
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(resp["results"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn track_endpoint_streams_bytes() {
        // Write a real temp file so the server can open it
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let content = b"fake audio bytes 1234";
        tmp.write_all(content).unwrap();
        tmp.flush().unwrap();

        let (server, db) = test_server();
        db.upsert_track(
            "Fake Track",
            "Test Artist",
            "",
            1000,
            tmp.path().to_str().unwrap(),
        )
        .unwrap();

        let row = db.find_track("Test Artist", "Fake Track").unwrap().unwrap();
        let mut body = Vec::new();
        ureq::get(&format!("{}/track/{}", base_url(&server), row.id))
            .call()
            .unwrap()
            .into_reader()
            .read_to_end(&mut body)
            .unwrap();

        assert_eq!(body, content);
    }

    #[test]
    fn track_endpoint_supports_range_request() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"0123456789").unwrap();
        tmp.flush().unwrap();

        let (server, db) = test_server();
        db.upsert_track("R", "A", "", 0, tmp.path().to_str().unwrap()).unwrap();
        let row = db.find_track("A", "R").unwrap().unwrap();

        let mut body = Vec::new();
        ureq::get(&format!("{}/track/{}", base_url(&server), row.id))
            .set("Range", "bytes=2-5")
            .call()
            .unwrap()
            .into_reader()
            .read_to_end(&mut body)
            .unwrap();

        assert_eq!(body, b"2345");
    }

    #[test]
    fn unknown_route_returns_404() {
        let (server, _db) = test_server();
        let resp = ureq::get(&format!("{}/nonexistent", base_url(&server))).call();
        assert!(matches!(resp, Err(ureq::Error::Status(404, _))));
    }
}
