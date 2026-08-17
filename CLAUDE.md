# CLAUDE.md — Deezer TUI Player

## Project Overview

Lightweight, self-contained Deezer music player in terminal. Rust, ~5-8 MB RAM, single static binary ~5 MB. No external media player (no mpv, vlc, ffmpeg).

## Architecture Principles

### Strict Separation: Core Library vs TUI Frontend

Two crates in Cargo workspace:

```
deezer-tui/
├── Cargo.toml              # Workspace root
├── CLAUDE.md
├── README.md
├── .circleci/config.yml    # CI/CD (CircleCI)
├── crates/
│   ├── deezer-core/        # Library crate — all business logic, no UI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── api/        # Deezer private API client
│   │       │   ├── mod.rs
│   │       │   ├── auth.rs       # ARL token + email/password login
│   │       │   ├── gateway.rs    # gw-light.php (getUserData, song.getData, etc.)
│   │       │   ├── media.rs      # media.deezer.com/v1/get_url
│   │       │   └── models.rs     # API response types (serde)
│   │       ├── decrypt.rs  # Blowfish CBC stripe decryption
│   │       ├── player/     # Audio playback engine
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs     # rodio/cpal playback, queue management
│   │       │   ├── stream.rs     # HTTP progressive streaming + decrypt
│   │       │   └── state.rs      # Player state (playing, paused, position, volume)
│   │       └── config.rs   # Configuration (credentials, quality prefs)
│   │
│   └── deezer-tui/         # Binary crate — terminal UI only
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs           # Entry point, fork logic (daemon/client)
│           ├── daemon.rs         # Background daemon (Unix socket server)
│           ├── client.rs         # TUI client (connects to daemon)
│           ├── protocol.rs       # IPC protocol (Command/ServerMessage enums)
│           ├── web_login.rs      # Browser-based OAuth login flow
│           ├── theme.rs          # Colors, styles, Deezer themes
│           └── ui/               # Ratatui rendering
│               ├── mod.rs
│               ├── login.rs      # Login screen
│               ├── player.rs     # Bottom player bar
│               ├── search.rs     # Search tab (multi-category)
│               ├── favorites.rs  # Favorites tab (multi-category)
│               ├── radio.rs      # Radios / Podcasts tab
│               ├── downloads.rs  # Downloads tab
│               ├── album_detail.rs # Album detail overlay
│               ├── popup.rs      # Context menus, modals, playlist picker
│               └── common.rs     # Shared widgets, Deezer logo pixel art
```

**Rule: `deezer-core` must NEVER depend on any TUI/UI crate.** Exposes clean async API any frontend (TUI, GUI, web, CLI) can consume.

**Rule: `deezer-tui` depends on `deezer-core`, handles rendering + input only.**

### Why This Separation Matters

- Swap TUI for native GUI (egui, iced, tauri) without touching audio/API logic
- Unit-test business logic independently from UI
- Potentially expose `deezer-core` as reusable crate

## Deezer API — How It Works

### Authentication

Deezer does NOT provide full-track streaming via public API (only 30s previews). Uses **private/undocumented API** (same as web player).

Three auth methods:
1. **ARL token** — 192-char cookie from logged-in browser session (easiest)
2. **Email/password** — MD5 hash + auth hash → obtain ARL programmatically
3. **Web browser login** — opens deezer.com in browser, intercepts ARL via URI handler (`web_login.rs`)

Auth flow:
1. Set ARL as cookie on `.deezer.com`
2. Call `deezer.getUserData` → get `api_token` (checkForm) + `license_token`
3. Tokens needed for all subsequent API calls

### Track Streaming Pipeline

```
1. song.getData(SNG_ID)        → TRACK_TOKEN, MD5_ORIGIN, metadata
2. media.deezer.com/v1/get_url → CDN streaming URL
   POST { license_token, track_tokens[], format: "MP3_128"|"MP3_320"|"FLAC" }
3. HTTP GET CDN URL             → encrypted audio stream
4. Blowfish CBC stripe decrypt  → raw audio (MP3 or FLAC)
5. symphonia decode             → PCM samples
6. rodio/cpal output            → speakers
```

### Audio Encryption (Blowfish CBC Stripe)

Custom (weak) encryption, NOT standard DRM:
- Algorithm: **Blowfish CBC** with fixed IV `\x00\x01\x02\x03\x04\x05\x06\x07`
- **Only every 3rd 2048-byte block** encrypted (blocks 0, 3, 6, 9…)
- Per-track key: `MD5(track_id)` XOR'd with master secret (16 bytes)
- Master secret extracted dynamically at runtime from Deezer's public web resources (NOT hardcoded)

Rust crates: `blowfish`, `cbc`, `md-5`

### Audio Quality Tiers

| Format   | Bitrate        | Requires        |
|----------|----------------|-----------------|
| MP3_128  | 128 kbps CBR   | Free account    |
| MP3_320  | 320 kbps CBR   | Premium         |
| FLAC     | ~1411 kbps     | HiFi / Premium+ |

Quality fallback: FLAC → MP3_320 → MP3_128 → MP3_64

## Cross-Platform Audio Output (No External Player)

Binary fully self-contained:

| Platform | Audio Backend | System Dependency          |
|----------|---------------|----------------------------|
| Linux    | ALSA          | `libasound2` (pre-installed on most distros) |
| macOS    | CoreAudio     | None (built into macOS)    |

Stack: `rodio` (high-level) → `cpal` (low-level, platform backends) → OS audio API

Audio decoding pure Rust via `symphonia` (MP3 + FLAC), no system codecs needed.

**Windows:** Not natively supported (Unix sockets, `fork()`, `libc` used for IPC). Windows users use **WSL2** (Windows 11) with Linux binary — audio works via WSLg/PulseAudio. Install `libasound2` in WSL.

## Key Dependencies

| Layer        | Crate                     | Purpose                                |
|--------------|---------------------------|----------------------------------------|
| Async        | `tokio`                   | Async runtime                          |
| HTTP         | `reqwest` + `cookie_store`| API calls, session management          |
| Streaming    | `stream-download`         | Progressive buffered HTTP download     |
| Crypto       | `blowfish`, `cbc`, `md-5` | Track decryption + key derivation      |
| Decoding     | `symphonia`               | Pure-Rust MP3/FLAC decoder             |
| Playback     | `rodio` (wraps `cpal`)    | Cross-platform audio output            |
| Serialization| `serde`, `serde_json`     | API response parsing                   |
| TUI          | `ratatui`, `crossterm`    | Terminal UI rendering                  |
| Images       | `ratatui-image`, `image`  | Album/artist art (Sixel, Kitty, iTerm2, halfblocks) |
| Config       | `directories`             | XDG/platform config paths              |
| IPC          | `tokio::net::UnixStream`  | Daemon/client socket communication     |
| Process      | `libc`                    | fork(), setsid(), dup2() (Unix only)   |

## UI Design

### Login Screen
- Shown when no ARL/credentials configured
- Deezer logo in pixel art (Unicode block characters)
- ARL token input or email/password fields
- Web browser login option (`w` key)

### Main Layout
```
┌──────────────────────────────────────────────────┐
│  [Search]  [Favorites]  [Radios]  [Downloads]   │  ← Tab bar
├──────────────────────────────────────────────────┤
│                                                  │
│              Content area                        │  ← Changes per tab
│              (lists, results, details)           │
│                                                  │
├──────────────────────────────────────────────────┤
│  ▶ Song Title — Artist      ━━━━━━━━━━━ 2:30    │  ← Player bar (always visible)
│  ◀◀  ▶/❚❚  ▶▶   🔊 ━━━━━   🔀  🔁             │
└──────────────────────────────────────────────────┘
```

### Overlays & Modals
- **Context menu** (`m`): Play next, Add to queue, Add to playlist, Start mix, Dislike
- **Queue / Waiting list** (`w`): View/manage play queue (delete, reorder, favorite)
- **Album detail** (`a`): Full track listing for album
- **Playlist detail** (`Enter` on playlist): Track listing for playlist
- **Shortcuts help** (`?`): Keyboard shortcuts reference
- **Settings** (`Ctrl+O`): Theme selection (official Deezer dark themes)

### Search & Favorites Categories
Both tabs support multi-category browsing (`h`/`l` to switch):
- **Search**: Tracks, Artists, Albums, Playlists, Podcasts, Episodes, Profiles
- **Favorites**: Recently Played, Tracks, Artists, Albums, Playlists, Following

## Reference Projects

- **[pleezer](https://github.com/roderickvd/pleezer)** — Rust headless Deezer Connect player. Proof full stack works. Study `decrypt`, `track`, `player` modules.
- **[deezer_downloader](https://github.com/zggff/deezer_downloader)** — Simpler Rust crate for download + decrypt.
- **[deemix](https://gitlab.com/deemix)** — Python, well-documented private API usage.
- **[dzr](https://github.com/yne/dzr)** — Shell/JS CLI player, good API reference.

## Master Key Extraction

Blowfish master secret extracted at runtime from Deezer's web player JavaScript:

1. Fetch `https://www.deezer.com/en/channels/explore/`
2. Regex-extract `app-web*.js` bundle URL
3. In JS, find two 8-byte URL-encoded hex arrays:
   - First half: starts `0x61`, ends `0x67` (regex: `0x61%2C(0x[0-9a-f]{2}%2C){6}0x67`)
   - Second half: starts `0x31`, ends `0x34` (regex: `0x31%2C(0x[0-9a-f]{2}%2C){6}0x34`)
4. Parse each half, reverse byte order, interleave: `a[0],b[0],a[1],b[1],...`
5. Validate via MD5: `7ebf40da848f4a0fb3cc56ddbe6c2d09`

Implemented in `deezer-core/src/decrypt.rs::fetch_master_key()`.

## Daemon / Client Architecture

**Daemon + client** model over Unix domain sockets:

```
┌─────────────────────────┐       Unix socket        ┌─────────────────────────┐
│      deezer-tui         │    (daemon.sock in        │      deezer-tui         │
│      (daemon)           │◄──  XDG config dir)  ───► │      (client/TUI)       │
│                         │                           │                         │
│  PlayerEngine (rodio)   │   JSON-line protocol:     │  Ratatui rendering      │
│  API client (reqwest)   │   Command ──────────►     │  Keyboard input         │
│  State management       │   ◄──────── Snapshot      │  ViewState (from snap)  │
│  tokio::spawn tasks     │                           │                         │
└─────────────────────────┘                           └─────────────────────────┘
```

### Startup Flow (Unix)
1. First invocation: `main.rs` calls `fork()` — child becomes daemon, parent becomes client
2. Subsequent invocations: detects existing `daemon.sock` → connects as client directly
3. `deezer-tui -q` / `--quit`: sends `Shutdown` command to daemon

### IPC Protocol (`protocol.rs`)
- **Transport**: Unix domain socket at `<config_dir>/daemon.sock`
- **Format**: Line-delimited JSON (newline-separated)
- **Client → Daemon**: `Command` enum (GetSnapshot, Search, PlayFromSearch, TogglePause, etc.)
- **Daemon → Client**: `ServerMessage::Snapshot(DaemonSnapshot)` — full state sent after every command + periodic ticks
- All snapshot fields use `#[serde(default)]` for forward/backward compatibility

### Async Architecture (inside Daemon)
Background tasks use `tokio::spawn`, results sent back via `mpsc::unbounded_channel`.
Daemon event loop uses `tokio::select!` to multiplex:
- Accepting new client connections
- Reading commands from client
- Processing async results (login, search, track ready, etc.)
- Periodic tick (position update, auto-advance)

PlayerEngine stays on daemon's main thread (rodio/cpal are `!Send`). Audio fetched+decrypted in background, played on main thread.

## Keyboard Shortcuts

### Global (Main Screen)
| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch tabs |
| `h` / `l` or `←` / `→` | Switch category within tab |
| `j` / `k` or `↑` / `↓` | Navigate list |
| `/` | Enter search mode (on Search tab) |
| `Enter` | Submit search / Play selected / Open detail |
| `Esc` | Close overlay / Exit search mode |
| `Space` | Play / Pause |
| `n` | Next track |
| `b` | Previous track |
| `s` | Toggle shuffle |
| `r` | Cycle repeat (Off → All → One) |
| `L` / `Ctrl+L` | Like / favorite track (focused or playing) |
| `+` / `-` | Volume up / down |
| `m` / `x` | Open context menu for selected track |
| `a` | Open album detail for selected track |
| `w` | Open waiting list (queue) |
| `?` | Show shortcuts help |
| `f` | Start Deezer Flow |
| `g` | Shuffle play favorites |
| `Ctrl+O` | Open settings (themes) |
| `q` | Quit (sends Shutdown to daemon) |
| `Ctrl+C` | Force quit |

### Queue / Waiting List (`w`)
| Key | Action |
|-----|--------|
| `d` / `Delete` | Remove track from queue |
| `L` / `f` | Toggle favorite |
| `x` | Open context menu |
| `Esc` / `w` | Close queue |

### Album / Playlist Detail
| Key | Action |
|-----|--------|
| `Enter` | Play track |
| `L` / `f` | Toggle favorite |
| `x` | Open context menu |
| `Esc` | Close detail |

## Development Guidelines

- Rust edition: 2021
- MSRV: 1.90+
- Error handling: `thiserror` for library errors, `anyhow` for binary
- Logging: `tracing` crate — daemon logs to `/tmp/deezer-daemon.log`, client to `/tmp/deezer-tui.log` (set `RUST_LOG=debug`)
- Tests: unit tests in `deezer-core`, integration tests for API (behind feature flag)
- CI: **CircleCI** — Linux x86_64, Linux aarch64, macOS universal (no Windows native build)
- Linting: `clippy` with pedantic warnings
- Formatting: `rustfmt` default — pre-commit hook in `.githooks/` auto-formats staged `.rs` files
- Config stored as JSON in XDG config dir (`~/.config/deezer-tui/config.json` on Linux)
- **Platform support**: Linux, macOS (native); Windows via WSL2 only (Unix sockets + fork required)

## Build & Verify Routine

After code changes, **always** run:

```bash
cargo fmt && cargo build --release
```

Ensures formatting correct (CI rejects unformatted code) and project compiles.
Run `cargo clippy -- -D warnings` for lint checks when relevant.

## Release Routine

1. Update `CHANGELOG.md`:
   - Move items from `[Unreleased]` into new version section `[X.Y.Z] - YYYY-MM-DD`
   - Categorize under `### Added`, `### Changed`, `### Fixed`, `### Removed`
   - Add fresh empty `[Unreleased]` section at top
2. Update version in `Cargo.toml` files (workspace root + both crates)
3. Run `cargo fmt && cargo build --release`
4. Commit and tag: `git tag vX.Y.Z`

When adding features or fixes (not during release), add brief entry under `[Unreleased]` in `CHANGELOG.md`.

## Legal Notice

Uses Deezer's undocumented private API for personal use. Users must have valid Deezer account. Master decryption secret NOT hardcoded — extracted at runtime from Deezer's public web resources, same as browser. Does not facilitate piracy — audio streamed, not saved.
