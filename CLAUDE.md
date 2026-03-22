# CLAUDE.md — Deezer TUI Player

## Project Overview

A lightweight, self-contained Deezer music player running entirely in the terminal.
Built with Rust for minimal memory footprint (~5-8 MB RAM) and a single static binary (~5 MB).
No external media player required (no mpv, vlc, ffmpeg).

## Architecture Principles

### Strict Separation: Core Library vs TUI Frontend

The codebase is split into two crates within a Cargo workspace:

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

**Rule: `deezer-core` must NEVER depend on any TUI/UI crate.**
It exposes a clean async API that any frontend (TUI, GUI, web, CLI) can consume.

**Rule: `deezer-tui` depends on `deezer-core` and only handles rendering + input.**

### Why This Separation Matters

- Swap the TUI for a native GUI (egui, iced, tauri) without touching audio/API logic
- Unit-test business logic independently from UI
- Potentially expose `deezer-core` as a reusable crate

## Deezer API — How It Works

### Authentication

Deezer does NOT provide full-track streaming via its public API (only 30s previews).
We use the **private/undocumented API** (same as the web player).

Three auth methods supported:
1. **ARL token** — a 192-char cookie from a logged-in browser session (easiest)
2. **Email/password** — MD5 hash + auth hash → obtain ARL programmatically
3. **Web browser login** — opens deezer.com in the user's browser, intercepts ARL via URI handler (`web_login.rs`)

Auth flow:
1. Set ARL as cookie on `.deezer.com`
2. Call `deezer.getUserData` → get `api_token` (checkForm) + `license_token`
3. These tokens are needed for all subsequent API calls

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

Deezer uses a custom (weak) encryption, NOT standard DRM:
- Algorithm: **Blowfish CBC** with fixed IV `\x00\x01\x02\x03\x04\x05\x06\x07`
- **Only every 3rd 2048-byte block** is encrypted (blocks 0, 3, 6, 9…)
- Per-track key derived from: `MD5(track_id)` XOR'd with a master secret (16 bytes)
- The master secret should be extracted dynamically at runtime from Deezer's public web resources (NOT hardcoded)

Rust crates: `blowfish`, `cbc`, `md-5`

### Audio Quality Tiers

| Format   | Bitrate        | Requires        |
|----------|----------------|-----------------|
| MP3_128  | 128 kbps CBR   | Free account    |
| MP3_320  | 320 kbps CBR   | Premium         |
| FLAC     | ~1411 kbps     | HiFi / Premium+ |

Quality fallback: FLAC → MP3_320 → MP3_128 → MP3_64

## Cross-Platform Audio Output (No External Player)

The binary is fully self-contained on all platforms:

| Platform | Audio Backend | System Dependency          |
|----------|---------------|----------------------------|
| Linux    | ALSA          | `libasound2` (pre-installed on most distros) |
| macOS    | CoreAudio     | None (built into macOS)    |

Stack: `rodio` (high-level) → `cpal` (low-level, platform backends) → OS audio API

Audio decoding is pure Rust via `symphonia` (MP3 + FLAC), no system codecs needed.

**Windows:** Not natively supported (Unix sockets, `fork()`, `libc` used for IPC).
Windows users should use **WSL2** (Windows 11) with the Linux binary — audio works
out of the box via WSLg/PulseAudio. Install `libasound2` in WSL.

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
- Shown when no ARL/credentials are configured
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
- **Queue / Waiting list** (`w`): View and manage the play queue (delete, reorder, favorite)
- **Album detail** (`a`): Full track listing for an album
- **Playlist detail** (`Enter` on playlist): Track listing for a playlist
- **Shortcuts help** (`?`): Keyboard shortcuts reference
- **Settings** (`Ctrl+O`): Theme selection (official Deezer dark themes)

### Search & Favorites Categories
Both tabs support multi-category browsing (`h`/`l` to switch):
- **Search**: Tracks, Artists, Albums, Playlists, Podcasts, Episodes, Profiles
- **Favorites**: Recently Played, Tracks, Artists, Albums, Playlists, Following

## Reference Projects

- **[pleezer](https://github.com/roderickvd/pleezer)** — Rust headless Deezer Connect player. Proof that this full stack works. Study its `decrypt`, `track`, and `player` modules.
- **[deezer_downloader](https://github.com/zggff/deezer_downloader)** — Simpler Rust crate for download + decrypt.
- **[deemix](https://gitlab.com/deemix)** — Python, well-documented private API usage.
- **[dzr](https://github.com/yne/dzr)** — Shell/JS CLI player, good API reference.

## Master Key Extraction

The Blowfish master secret is extracted at runtime from Deezer's web player JavaScript:

1. Fetch `https://www.deezer.com/en/channels/explore/`
2. Regex-extract the `app-web*.js` bundle URL
3. In the JS, find two 8-byte URL-encoded hex arrays:
   - First half: starts `0x61`, ends `0x67` (regex: `0x61%2C(0x[0-9a-f]{2}%2C){6}0x67`)
   - Second half: starts `0x31`, ends `0x34` (regex: `0x31%2C(0x[0-9a-f]{2}%2C){6}0x34`)
4. Parse each half, reverse byte order, interleave: `a[0],b[0],a[1],b[1],...`
5. Validate via MD5: `7ebf40da848f4a0fb3cc56ddbe6c2d09`

Implemented in `deezer-core/src/decrypt.rs::fetch_master_key()`.

## Daemon / Client Architecture

The application uses a **daemon + client** model over Unix domain sockets:

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
- All snapshot fields use `#[serde(default)]` for forward/backward compatibility between versions

### Async Architecture (inside Daemon)
Background tasks use `tokio::spawn` with results sent back via `mpsc::unbounded_channel`.
The daemon event loop uses `tokio::select!` to multiplex:
- Accepting new client connections
- Reading commands from client
- Processing async results (login, search, track ready, etc.)
- Periodic tick (position update, auto-advance)

PlayerEngine stays on the daemon's main thread (rodio/cpal are `!Send`).
Audio data is fetched+decrypted in background, then played on main thread.

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
| `+` / `-` | Volume up / down |
| `m` | Open context menu for selected track |
| `a` | Open album detail for selected track |
| `w` | Open waiting list (queue) |
| `?` | Show shortcuts help |
| `g` | Shuffle play favorites |
| `Ctrl+O` | Open settings (themes) |
| `Ctrl+F` | Toggle fullscreen |
| `Ctrl+P` | Open command palette |
| `q` | Quit (sends Shutdown to daemon) |
| `Ctrl+C` | Force quit |

### Queue / Waiting List (`w`)
| Key | Action |
|-----|--------|
| `d` / `Delete` | Remove track from queue |
| `f` | Toggle favorite |
| `m` | Open context menu |
| `Esc` / `w` | Close queue |

### Album / Playlist Detail
| Key | Action |
|-----|--------|
| `Enter` | Play track |
| `m` | Open context menu |
| `Esc` | Close detail |

## Development Guidelines

- Rust edition: 2021
- MSRV: 1.90+
- Error handling: `thiserror` for library errors, `anyhow` for binary
- Logging: `tracing` crate — daemon logs to `/tmp/deezer-daemon.log`, client to `/tmp/deezer-tui.log` (set `RUST_LOG=debug`)
- Tests: unit tests in `deezer-core`, integration tests for API (behind feature flag)
- CI: **CircleCI** — Linux x86_64, Linux aarch64, macOS universal (no Windows native build)
- Linting: `clippy` with pedantic warnings
- Formatting: `rustfmt` with default settings — a pre-commit hook in `.githooks/` auto-formats staged `.rs` files
- Config stored as JSON in XDG config dir (`~/.config/deezer-tui/config.json` on Linux)
- **Platform support**: Linux, macOS (native); Windows via WSL2 only (Unix sockets + fork required)

## Build & Verify Routine

After making code changes, **always** run:

```bash
cargo fmt && cargo build --release
```

This ensures formatting is correct (CI will reject unformatted code) and the project compiles.
Run `cargo clippy -- -D warnings` for additional lint checks when relevant.

## Legal Notice

This project uses Deezer's undocumented private API for personal use.
Users must have a valid Deezer account. The master decryption secret is NOT hardcoded;
it is extracted at runtime from Deezer's public web resources, same as a browser would.
This project does not facilitate piracy — audio is streamed, not downloaded/saved.
