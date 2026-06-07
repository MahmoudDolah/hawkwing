# Hawkwing

A ground-up Rust rewrite of [Tomahawk](https://github.com/tomahawk-player/tomahawk) — a music player that decouples song metadata from playback sources. Given an artist and title, Hawkwing resolves where to play it (local library, streaming services, P2P friends) and plays it.

**Status:** Phase 1 — local library scan and playback from the CLI.

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
hawkwing [--db <path>] <command>
```

The database defaults to `~/.local/share/hawkwing/library.db`.

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
├── resolver/
│   ├── mod.rs       # Resolver trait + Orchestrator
│   └── local.rs     # LocalResolver — hits SQLite
└── audio.rs         # Player wrapping rodio (feature-gated)
```

### Resolver protocol

The `Resolver` trait is designed to be implemented by multiple backends. The `Orchestrator` fans out to all registered resolvers, merges results, and picks the highest-scoring match. Phase 1 ships with only `LocalResolver`; later phases add HTTP subprocess resolvers and P2P.

---

## Roadmap

- **Phase 2:** HTTP REST resolver protocol — language-agnostic, subprocess-isolated resolver plugins
- **Phase 3:** P2P discovery via mDNS + direct TCP streaming
- **Phase 4:** Qt6/QML desktop UI
