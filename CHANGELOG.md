# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Context menu on a playlist (`x` shortcut), with "Rename" and "Delete" actions
- Create a new playlist from the "Add to playlist" modal

### Changed
- Playlist picker: only personal and collaborative playlists shown
- Context menu on a track inside a playlist detail shows "Remove from playlist" instead of "Add to playlist"

### Fixed
- Playlist picker GATEWAY_ERROR when adding a track to a playlist
- Adding a track already present in a playlist now shows a friendly notification instead of the raw API error

## [1.10.0]

### Added
- Similar artists section on the artist detail page
- Theme background transparency (#4)

### Changed
- "<< Back" navigation hint on artist/album pages (replaces tab bar)
- Reduced RAM footprint
- Improved artist/album page headers
- Improved offline display
- Improved playlist `[w]` shortcut

### Fixed
- Command line `-n`/`-b`/`-p` no longer crash (#5)
- Artist/album pages behaviors when a modal is displayed on top
- Album/artist left column focus on large windows
- Volume persisted across restarts
- Help modal scroll

## [1.9.0] - 2026-04-11

### Added
- Fuzzy filter for favorites and radios (#3)
- Favorites cache : speed up navigation (#2)

## [1.8.1] - 2026-04-10

### Added
- album and artist page left column scroll

### Changed
- improve album and artsit miniatures responsiveness

### Fixed
- deezer-tui core behavior on update
- API error : remove an artist from favorites

## [1.8.0] - 2026-03-31

### Added
- Deezer Flow support with `[f]` shortcut
- Add waiting list "Enter" event
- Track Forward `ctrl + 🠆` and backward `ctrl + 🠄`
- Project changelog

### Changed
- Volume display moved next to progress bar for cleaner layout

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
