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

## Project Phases

- **Phase 1** (complete): scan library, search, play local files from CLI
- **Phase 2**: HTTP REST resolver protocol + subprocess-isolated resolvers
- **Phase 3**: P2P discovery via mDNS + direct TCP
- **Phase 4**: Qt6/QML UI

## Reference Codebase

The original Tomahawk lives at `/home/mood/Documents/git-repos/tomahawk` — use it as a
design reference only, not code to compile. Key files for future phases:
- `src/libtomahawk/database/` — command pattern worth porting as mutation log for P2P sync
- `src/libtomahawk/network/Servent.cpp` — read before designing Phase 3 networking
- `src/libtomahawk/resolvers/JSResolver.cpp` — every `addToJavaScriptWindowObject` call is a
  feature the resolver protocol must expose
