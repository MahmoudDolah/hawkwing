# Hawkwing — Claude Code Guide

## Build & Test

```bash
# Default (no audio — no system deps required)
cargo build
cargo test

# With audio playback enabled (requires libasound2-dev)
cargo build --features audio
cargo test --features audio
```

## Key Design Decisions

### Audio is an optional feature
`rodio` (via `cpal`) requires `libasound2-dev` at compile time on Linux. The `audio` feature
gates all rodio code so that `cargo test` works without any system audio headers. At runtime,
audio routes through PipeWire via its ALSA compatibility layer (`pipewire-alsa` is installed).

### `Database` wraps `Mutex<Connection>`
`rusqlite::Connection` is `Send` but not `Sync`. The `Resolver` trait requires `Send + Sync`
(needed for future multi-threaded resolver fan-out). Wrapping in `Mutex` satisfies this.
Do not remove the `Mutex` or attempt to use `RefCell` here.

### `ResolveResult` not `Result`
The domain type for a resolved track is named `ResolveResult` to avoid clashing with
`anyhow::Result` / `std::result::Result` in the same files.

### Resolver trait
```rust
pub trait Resolver: Send + Sync {
    fn name(&self) -> &str;
    fn weight(&self) -> u8;   // higher = tried first
    fn resolve(&self, query: &Query) -> anyhow::Result<Vec<ResolveResult>>;
}
```
`Orchestrator` fans out to all resolvers, merges, and sorts descending by `score: f32`.

### Resolver protocol (Phase 2)
Resolver executables are spawned as subprocesses (`ResolverProcess`), print `{"port": N}`
on their first stdout line, and expose an HTTP API: `GET /info`, `POST /resolve`,
`GET /track/<id>`. `HttpResolver` wraps either a spawned process or a peer URL.
`load_resolvers(dir)` scans a directory for executables and connects to each.

### P2P (Phase 3)
`--p2p` starts a `Node`: a `PeerServer` (tiny_http, serves `/info`, `/resolve`,
`/track/<id>` with Range support) plus mDNS announce/browse (`mdns-sd`,
service type `_hawkwing._tcp.local.`). Discovered peers connect via
`HttpResolver::connect_peer`, which is just an `HttpResolver` pointed at the
peer's base URL — no separate peer-resolver type was needed. Peer servers don't
know their own external address, so they return relative URLs (`/track/<id>`);
`HttpResolver::resolve` detects the leading `/` and prepends `base_url`.
A UUID v4 node ID is persisted at `~/.local/share/hawkwing/node_id` (`node_id::get_or_create`).

## Project Phases

- **Phase 1** (complete): scan library, search, play local files from CLI
- **Phase 2** (complete): HTTP REST resolver protocol + subprocess-isolated resolvers
- **Phase 3** (complete): P2P discovery via mDNS + direct TCP
- **Phase 4**: Qt6/QML UI

## Reference Codebase

The original Tomahawk lives at `/home/mood/Documents/git-repos/tomahawk` — use it as a
design reference only, not code to compile. Key files for future phases:
- `src/libtomahawk/database/` — command pattern worth porting as mutation log for P2P sync
- `src/libtomahawk/network/Servent.cpp` — read before designing Phase 3 networking
- `src/libtomahawk/resolvers/JSResolver.cpp` — every `addToJavaScriptWindowObject` call is a
  feature the resolver protocol must expose
