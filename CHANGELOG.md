# Changelog

All notable changes to this project will be documented in this file.

## [1.15.2] - 2026-08-06

### Fixed
- Opening a private playlist is not working (#17)
- Previous track under shuffle now steps back through the tracks actually
  played, instead of jumping to the queue's preceding track

### Changed
- Shuffle is now a full pass over the queue — each track plays once per cycle —
  instead of an endless random walk.
- Enabling shuffle now turns repeat off. Shuffle + repeat can still be set by enabling shuffle first
- On "Repeat one" loops on current track + next/prev will resumes the shuffle
  cycle if shuffle is on

## [1.15.1] - 2026-08-06

### Fixed
- Client/daemon IPC desync causing "Communication error" JSON parse failures (#16, #19)
- Hardcoded French "titres" in the Albums and Playlists track-count column (#18)
- `r` (repeat) and `s` (shuffle) were ignored in every overlays (album, artist, playlist, podcast, genre, offline detail, waiting list) (#15)
- Private and collaborative playlists were missing from the Playlists tab (#17)
- The cursor now moves inside the viewport, which only scrolls once the cursor reaches an edge (#20)
- Expired pipe.deezer.com JWT is now refreshed and the call retried once, instead of surfacing "JWT token has expired, please reauthenticate" (#14)

## [1.15.0] - 2026-07-26

### Added
- Mouse support: 
    * click on menus, tabs, search bar...
    * scroll lists
    * can click `Flow` and `Shuffle play my favorite`
    * escape modales clicking background
    * right click to open track context menu
    * right click on bottom bar to open playing track's context menu
- Download a whole playlist for offline mode (from context menu `x`) with % progression (#13)
- New Playlists Offline tab 
- Offline filters for tracks, albums and playlists
- Offline context menu : can now remove a track, album or playlist from offline storage
- Search > Episodes, and Search > Podcasts is now working

### Changed
- Keyboard shortcut indicators now share same color and style everywhere in the app
- Background transparency is now a simple on/off toggle
- Improve album and artist page back button adding location indicators
- Modals no longer overlap the player bar
- Status notifications moved to the player bar's top and auto disappear after 7 seconds
- Removed the top header line (app title + status area)
- Offline albums and playlists tracks are now displayed in modals
- Track lists mark the playing track with a `●` bullet instead of `▶`, no longer confusable with the `>` cursor

### Fixed
- Opening a modal/overlay now dims the background
- The player bar now keeps its colors when an overlay is open
- Playlist and waiting-list modals now dim the background
- On image-capable terminals, closing a modal that overlapped the cover image no longer leaves fragments
- Favorites > Following page content

## [1.14.0] - 2026-07-19

### Added
- Fuzzy search (`/`) in the "Add to playlist" picker (#10)
- Repeat mode indicator (`[r]`) in the player bar

### Fixed
- Shuffle mode could get stuck unable to disable once MPRIS was active, due to a stale state reference kept after login (#11)
- Shuffle no longer replays the same track twice in a row, including right after the queue is extended
- Playlist contents were cached forever and never picked up tracks added from another device; playlists now refresh in the background on reopen (#12)
- Track count next to a playlist (Favorites > Playlists, and the "Add to playlist" picker) didn't update after tracks were added/removed from another device, even after the playlist's own track list had refreshed
- Deezer Flow could skip ahead in the stream instead of continuing sequentially when the loaded queue ran out
- Moods played only the first batch of tracks and then stopped instead of continuing
- Favorites > Recently Played header now shows the track count
- Playlist picker: `?` and `i` shortcuts no longer interrupt typing in the filter field

## [1.13.0] - 2026-06-27

### Added
- MPRIS support on Linux: hardware media keys (play/pause, next, previous) and desktop now-playing widgets (GNOME, KDE, `playerctl`, …) can control and observe playback (#9)

## [1.12.1] - 2026-05-20

### Fixed
- Favorites > Recently Played now shows actual play history in reverse-chronological order
- Tracks played in deezer-tui now show up in your Deezer listening history (Favorites > Recently Played and your profile on deezer.com), after ~30 s of playback

## [1.12.0] - 2026-05-19

### Changed
- Renamed `Radios` tab to `Explore`

### Added
- Explore > `Moods` (new)
- Explore > `Categories` (new), with detail page (Enter on a music category)
- Explore > `Radios` (previously Radios tab)
- Audio quality picker in settings (`Ctrl+O` > Audio quality): MP3 64 / 128 / 320 / FLAC

### Fixed
- Favorites > Recently Played failing to load when list contains user-uploaded MP3 tracks (#7)
- `quality` field in `config.json` now accepts the API names (`MP3_128`, `MP3_320`, `FLAC`, `MP3_64`), as shown in the player bar, and case-insensitive. Previously only the internal PascalCase form was accepted, causing the whole config to revert to defaults on parse failure (#8)

## [1.11.0] - 2026-05-06

### Added
- Context menu on a playlist (`x` shortcut), with "Rename" and "Delete" actions
- Create a new playlist from the "Add to playlist" modal

### Changed
- Playlist picker: only personal and collaborative playlists shown
- Context menu on a track inside a playlist detail shows "Remove from playlist" instead of "Add to playlist"

### Fixed
- Playlist picker GATEWAY_ERROR when adding a track to a playlist
- Adding a track already present in a playlist now shows a friendly notification instead of the raw API error

## [1.10.0] - 2026-04-15

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
