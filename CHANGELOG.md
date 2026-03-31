# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Deezer Flow support with `[f]` shortcut (personalized radio)
- Flow button in footer bar with accent color
- Volume display moved next to progress bar for cleaner layout
- Project changelog

## [1.7.0] - 2026-03-31

### Added
- Auto-update mechanism for deezer-tui binary

## [1.6.0] - 2026-03-30

### Added
- Navigation history to recover overlay state after reconnecting
- Album/artist context menu
- Better keyboard shortcuts

### Fixed
- Time label background rendering over progress bar
- Quit shortcut now works from any page
- Halfblock miniatures noise artifacts

## [1.5.2] - 2026-03-22

### Fixed
- CircleCI release pipeline

## [1.5.1] - 2026-03-22

### Fixed
- Rust version compatibility

## [1.5.0] - 2026-03-22

### Added
- Artist detail page
- Album and artist miniatures (cover art)
- Command line options (`-q`/`--quit`)

### Fixed
- Quit shortcut behavior

## [1.4.0] - 2026-03-22

### Added
- Offline track mode (download and play without internet)
- Notifications moved to top status bar

### Fixed
- Offline track playing and UI navigation

## [1.3.0] - 2026-03-21

### Added
- Release script and version display in app info

## [1.2.0] - 2026-03-21

### Added
- Radio tab with Deezer radio stations
- Internationalization (i18n) with multiple language support
- Install script (`install.sh`)

### Fixed
- Some tracks not playing or loading slowly
- Next/previous track behavior when paused
- Status bar translations

## [1.1.1] - 2026-03-19

### Fixed
- Code formatting (`cargo fmt --check` compliance)

## [1.1.0] - 2026-03-19

### Added
- Waiting list (queue) overlay
- Playlist picker modal
- Album detail page with track listing
- Multi-category search (tracks, artists, albums, playlists, podcasts, episodes, profiles)

### Fixed
- Favorite sub-menu behavior
- Sudden UI exit crash

## [1.0.2] - 2026-03-12

### Changed
- Removed Windows from release targets (Unix-only: Linux, macOS)

## [1.0.1] - 2026-03-12

### Fixed
- CircleCI default Rust/Cargo version
- Code formatting checks

## [1.0.0] - 2026-03-06

### Added
- Initial release
- Deezer private API integration (ARL token + web browser login)
- Full audio streaming pipeline (Blowfish CBC decryption, symphonia decoding, rodio playback)
- Daemon/client architecture over Unix domain sockets
- Search with multi-category results
- Favorites management
- Track context menu (play next, add to queue, add to playlist, dislike, share)
- Settings menu with Deezer dark themes (Crimson, Emerald, Amber, Magenta, Halloween, etc.)
- Keyboard shortcuts help modal
- CircleCI build pipeline (Linux x86_64, Linux aarch64, macOS universal)
