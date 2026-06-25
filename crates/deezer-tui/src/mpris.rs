//! MPRIS D-Bus integration (Linux only).
//!
//! Exposes the daemon as an `org.mpris.MediaPlayer2.deezer_tui` player so that
//! hardware media keys (play/pause, next, previous) and desktop "now playing"
//! widgets (GNOME, KDE, `playerctl`, …) can control and observe playback.
//!
//! Design: the [`MprisHandler`] is a thin adapter. Method calls coming from the
//! desktop are translated into [`Command`]s and forwarded to the daemon over an
//! mpsc channel (so they go through the exact same code path as the TUI client
//! and the `-p`/`-n`/`-b` CLI flags). Property *reads* are answered directly
//! from the shared [`PlayerState`]. The daemon proactively emits
//! `PropertiesChanged` via [`Server::properties_changed`] when state changes.

use std::sync::{Arc, Mutex};

use mpris_server::{
    zbus::fdo, LoopStatus, Metadata, PlaybackRate, PlaybackStatus as MprisStatus, Property, Time,
    TrackId, Volume,
};

use deezer_core::player::state::{PlaybackStatus, PlayerState, RepeatMode};

use crate::protocol::Command;

/// Bus name suffix → `org.mpris.MediaPlayer2.deezer_tui`.
/// Must be a valid D-Bus name element (no hyphens allowed).
pub const BUS_SUFFIX: &str = "deezer_tui";

/// Adapter implementing the MPRIS `Root` and `Player` D-Bus interfaces.
pub struct MprisHandler {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<Command>,
    state: Arc<Mutex<PlayerState>>,
}

impl MprisHandler {
    pub fn new(
        cmd_tx: tokio::sync::mpsc::UnboundedSender<Command>,
        state: Arc<Mutex<PlayerState>>,
    ) -> Self {
        Self { cmd_tx, state }
    }

    fn send(&self, cmd: Command) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn status(&self) -> PlaybackStatus {
        self.state.lock().map(|s| s.status).unwrap_or_default()
    }
}

/// Build cover-art URL from a Deezer `ALB_PICTURE` md5 hash.
fn cover_url(album_picture: &str) -> Option<String> {
    if album_picture.is_empty() {
        return None;
    }
    Some(format!(
        "https://e-cdns-images.dzcdn.net/images/cover/{album_picture}/264x264-000000-80-0-0.jpg"
    ))
}

/// Snapshot of the fields we mirror onto D-Bus, used to detect changes and emit
/// `PropertiesChanged` only for what actually changed.
#[derive(Clone, Default, PartialEq)]
pub struct MprisSnapshot {
    track_id: Option<String>,
    status: PlaybackStatus,
    volume_milli: i64,
    has_track: bool,
}

impl MprisSnapshot {
    /// Capture the MPRIS-relevant fields from the current player state.
    pub fn capture(state: &PlayerState) -> Self {
        Self {
            track_id: state.current_track.as_ref().map(|t| t.track_id.clone()),
            status: state.status,
            volume_milli: (state.volume * 1000.0).round() as i64,
            has_track: state.current_track.is_some(),
        }
    }

    /// Diff against the previously emitted snapshot, returning the list of
    /// MPRIS properties that need a `PropertiesChanged` signal.
    pub fn changed_properties(&self, prev: &Self, state: &PlayerState) -> Vec<Property> {
        let mut props = Vec::new();
        if self.track_id != prev.track_id {
            props.push(Property::Metadata(build_metadata(state)));
        }
        if self.status != prev.status {
            props.push(Property::PlaybackStatus(to_mpris_status(self.status)));
        }
        if self.volume_milli != prev.volume_milli {
            props.push(Property::Volume(state.volume as Volume));
        }
        if self.has_track != prev.has_track {
            props.push(Property::CanGoNext(self.has_track));
            props.push(Property::CanGoPrevious(self.has_track));
            props.push(Property::CanPause(self.has_track));
            props.push(Property::CanPlay(self.has_track));
            props.push(Property::CanSeek(self.has_track));
        }
        props
    }
}

fn to_mpris_status(status: PlaybackStatus) -> MprisStatus {
    match status {
        PlaybackStatus::Playing | PlaybackStatus::Loading => MprisStatus::Playing,
        PlaybackStatus::Paused => MprisStatus::Paused,
        PlaybackStatus::Stopped => MprisStatus::Stopped,
    }
}

fn build_metadata(state: &PlayerState) -> Metadata {
    let mut builder = Metadata::builder();
    if let Some(track) = state.current_track.as_ref() {
        // mpris:trackid must be a valid D-Bus object path.
        if let Ok(id) = TrackId::try_from(format!("/org/deezer_tui/track/{}", track.track_id)) {
            builder = builder.trackid(id);
        }
        builder = builder
            .title(track.title.clone())
            .artist([track.artist.clone()]);
        if !track.album.is_empty() {
            builder = builder.album(track.album.clone());
        }
        if let Some(url) = cover_url(&track.album_picture) {
            builder = builder.art_url(url);
        }
    }
    if state.duration_secs > 0 {
        builder = builder.length(Time::from_secs(state.duration_secs as i64));
    }
    builder.build()
}

impl mpris_server::RootInterface for MprisHandler {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        self.send(Command::Shutdown);
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("Deezer TUI".into())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("deezer-tui".into())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

impl mpris_server::PlayerInterface for MprisHandler {
    async fn next(&self) -> fdo::Result<()> {
        self.send(Command::NextTrack);
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        self.send(Command::PrevTrack);
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        if self.status() == PlaybackStatus::Playing {
            self.send(Command::TogglePause);
        }
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        self.send(Command::TogglePause);
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        self.send(Command::Stop);
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        if self.status() == PlaybackStatus::Paused {
            self.send(Command::TogglePause);
        }
        Ok(())
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let secs = offset.as_secs();
        if secs >= 0 {
            self.send(Command::SeekForward { secs: secs as u64 });
        } else {
            self.send(Command::SeekBackward {
                secs: secs.unsigned_abs(),
            });
        }
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        let secs = position.as_secs().max(0) as u64;
        self.send(Command::SeekAbsolute { secs });
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<MprisStatus> {
        Ok(to_mpris_status(self.status()))
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        let repeat = self.state.lock().map(|s| s.repeat).unwrap_or_default();
        Ok(match repeat {
            RepeatMode::Off => LoopStatus::None,
            RepeatMode::Track => LoopStatus::Track,
            RepeatMode::Queue => LoopStatus::Playlist,
        })
    }

    async fn set_loop_status(&self, loop_status: LoopStatus) -> mpris_server::zbus::Result<()> {
        let target = match loop_status {
            LoopStatus::None => RepeatMode::Off,
            LoopStatus::Track => RepeatMode::Track,
            LoopStatus::Playlist => RepeatMode::Queue,
        };
        // Cycle order is Off -> Queue -> Track -> Off.
        let mut current = self.state.lock().map(|s| s.repeat).unwrap_or_default();
        for _ in 0..3 {
            if current == target {
                break;
            }
            self.send(Command::CycleRepeat);
            current = match current {
                RepeatMode::Off => RepeatMode::Queue,
                RepeatMode::Queue => RepeatMode::Track,
                RepeatMode::Track => RepeatMode::Off,
            };
        }
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.state.lock().map(|s| s.shuffle).unwrap_or(false))
    }

    async fn set_shuffle(&self, shuffle: bool) -> mpris_server::zbus::Result<()> {
        let current = self.state.lock().map(|s| s.shuffle).unwrap_or(false);
        if current != shuffle {
            self.send(Command::ToggleShuffle);
        }
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let state = self
            .state
            .lock()
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok(build_metadata(&state))
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.state.lock().map(|s| s.volume as Volume).unwrap_or(1.0))
    }

    async fn set_volume(&self, volume: Volume) -> mpris_server::zbus::Result<()> {
        self.send(Command::SetVolume {
            volume: volume as f32,
        });
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        let secs = self.state.lock().map(|s| s.position_secs).unwrap_or(0);
        Ok(Time::from_secs(secs as i64))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(self
            .state
            .lock()
            .map(|s| s.current_track.is_some())
            .unwrap_or(false))
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(self
            .state
            .lock()
            .map(|s| s.current_track.is_some())
            .unwrap_or(false))
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(self
            .state
            .lock()
            .map(|s| s.current_track.is_some())
            .unwrap_or(false))
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}
