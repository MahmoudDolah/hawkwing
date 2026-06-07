# Hawkwing

A ground-up Rust rewrite of [Tomahawk](https://github.com/tomahawk-player/tomahawk) — a music player that decouples song metadata from playback sources. Given an artist and title, Hawkwing resolves where to play it (local library, streaming services, P2P friends) and plays it.

**Status:** Phase 3 complete — local library scan/playback, HTTP resolver plugins, and P2P discovery via mDNS.

---

## System Prerequisites

| Dependency | Purpose | Required |
|---|---|---|
| `libasound2-dev` | ALSA headers for audio compilation | Only for `--features audio` |

```bash
sudo apt-get install libasound2-dev   # Debian/Ubuntu
sudo dnf install alsa-lib-devel       # Fedora
```

Audio output routes through PipeWire at runtime (via `pipewire-alsa`). The ALSA headers are a compile-time-only requirement.

---

## Build

```bash
# Library + CLI, no audio playback
cargo build

# Full build including audio playback
cargo build --features audio
```

---

## Usage

```
hawkwing [--db <path>] [--resolvers <dir>] [--p2p] <command>
```

The database defaults to `~/.local/share/hawkwing/library.db`.
Resolver executables default to `~/.local/share/hawkwing/resolvers/`.
`--p2p` announces this node over mDNS, runs a peer HTTP server, and adds
discovered peers as resolvers (only takes effect for `play`).

### Commands

```bash
# Index a music directory
hawkwing scan ~/Music

# Full-text search across title, artist, and album
hawkwing search "dark side"

# List all indexed tracks
hawkwing list

# Resolve and play a track (requires --features audio build)
hawkwing play "Pink Floyd" "Money"

# Resolve and play, including tracks shared by peers on the local network
hawkwing play --p2p "Pink Floyd" "Money"
```

---

## Development

```bash
cargo test                    # all tests, no system deps
cargo test --features audio   # include audio module
```

---

## Architecture

```
src/
├── main.rs          # clap CLI entry point
├── lib.rs           # module declarations
├── db/
│   ├── mod.rs       # Database (SQLite via rusqlite, FTS5 search)
│   └── schema.sql   # schema + triggers embedded at compile time
├── scanner.rs       # walk directory, read tags with lofty
├── query.rs         # Query, ResolveResult, Source types
├── node_id.rs       # persistent UUID v4 node identity
├── resolver/
│   ├── mod.rs       # Resolver trait + Orchestrator + load_resolvers
│   ├── local.rs     # LocalResolver — hits SQLite
│   ├── process.rs   # ResolverProcess — spawns + supervises subprocess resolvers
│   └── http.rs      # HttpResolver — REST client for subprocess & peer resolvers
├── p2p/
│   ├── mod.rs       # Node — owns PeerServer + mDNS daemon, discovers peers
│   ├── discovery.rs # mDNS announce/browse (_hawkwing._tcp.local.)
│   └── server.rs    # PeerServer — exposes /info, /resolve, /track/<id>
└── audio.rs         # Player wrapping rodio (feature-gated)
```

### Resolver protocol

The `Resolver` trait is designed to be implemented by multiple backends. The `Orchestrator` fans out to all registered resolvers, merges results, and picks the highest-scoring match.

- **`LocalResolver`** (weight 100) — exact match against the local SQLite library.
- **HTTP subprocess resolvers** (Phase 2) — `load_resolvers()` scans a directory for
  executables, spawns each (`ResolverProcess`), and wraps it in an `HttpResolver` that
  speaks a small REST protocol (`GET /info`, `POST /resolve`, `GET /track/<id>`).
- **Peer resolvers** (Phase 3) — `--p2p` discovers other Hawkwing nodes on the LAN via
  mDNS and connects to each with `HttpResolver::connect_peer`, reusing the same REST
  protocol that subprocess resolvers speak (peer = `PeerServer` exposing the identical API).

### P2P (Phase 3)

Each node persists a UUID v4 identity (`node_id`) and, when started with `--p2p`:

- runs a `PeerServer` (HTTP, thread-per-request) that serves the local library to peers,
  including byte-range requests for seeking/streaming;
- announces itself over mDNS as `_hawkwing._tcp.local.` and browses for other nodes;
- adds each discovered peer as a resolver (weight 80, below `LocalResolver`'s 100), so
  local matches are always preferred over network ones.

Peer audio is downloaded to a temp file and played the same way as local files.

---

## Roadmap

- **Phase 1:** Local library scan, FTS5 search, playback from the CLI ✅
- **Phase 2:** HTTP REST resolver protocol — language-agnostic, subprocess-isolated resolver plugins ✅
- **Phase 3:** P2P discovery via mDNS + direct TCP streaming ✅
- **Phase 4:** Qt6/QML desktop UI
