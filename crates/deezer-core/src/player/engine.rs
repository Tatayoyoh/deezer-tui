use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use tracing::{debug, info};

use crate::api::models::{AudioQuality, DeezerError, TrackData};
use crate::player::eq::{EqHandle, Equalizer};
use crate::player::state::{PlaybackStatus, PlayerState};
use crate::player::stream::StreamReader;

/// Audio handed to the engine: a fully downloaded file (offline tracks,
/// podcast episodes) or a track still streaming from the CDN.
pub enum AudioInput {
    Bytes(Cursor<Vec<u8>>),
    Stream(StreamReader),
}

impl From<Vec<u8>> for AudioInput {
    fn from(data: Vec<u8>) -> Self {
        Self::Bytes(Cursor::new(data))
    }
}

impl From<StreamReader> for AudioInput {
    fn from(reader: StreamReader) -> Self {
        Self::Stream(reader)
    }
}

impl AudioInput {
    /// Size for logs: the full length for downloaded audio, `None` while streaming.
    pub fn known_len(&self) -> Option<usize> {
        match self {
            Self::Bytes(c) => Some(c.get_ref().len()),
            Self::Stream(_) => None,
        }
    }
}

impl Read for AudioInput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Bytes(c) => c.read(buf),
            Self::Stream(s) => s.read(buf),
        }
    }
}

impl Seek for AudioInput {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            Self::Bytes(c) => c.seek(pos),
            Self::Stream(s) => s.seek(pos),
        }
    }
}

pub struct PlayerEngine {
    state: Arc<Mutex<PlayerState>>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Sink,
    eq: EqHandle,
}

impl PlayerEngine {
    /// Builds the engine on top of an existing shared state handle, so callers that
    /// already handed out `Arc` clones (e.g. MPRIS) keep seeing live updates instead
    /// of a snapshot frozen before the engine existed. `eq` is likewise shared:
    /// settings applied to it are heard live on the playing track.
    pub fn new(
        _master_key: [u8; 16],
        state: Arc<Mutex<PlayerState>>,
        eq: EqHandle,
    ) -> Result<Self, DeezerError> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| DeezerError::Playback(e.to_string()))?;

        let sink =
            Sink::try_new(&stream_handle).map_err(|e| DeezerError::Playback(e.to_string()))?;

        Ok(Self {
            state,
            _stream: stream,
            stream_handle,
            sink,
            eq,
        })
    }

    pub fn state(&self) -> Arc<Mutex<PlayerState>> {
        Arc::clone(&self.state)
    }

    /// Play decrypted audio: fully downloaded, or still streaming in.
    /// Called on the main thread with audio from a background fetch.
    pub fn play_decoded(
        &mut self,
        audio: impl Into<AudioInput>,
        track: &TrackData,
        quality: AudioQuality,
    ) -> Result<(), DeezerError> {
        let source = Decoder::new(audio.into())
            .map_err(|e| DeezerError::Playback(format!("Failed to decode audio: {e}")))?;
        let source = Equalizer::new(source, self.eq.clone());

        // Preserve current volume before recreating the sink
        let current_volume = self.state.lock().unwrap().volume;

        // Clear the current sink and create a fresh one (Sink can't be reused after stop)
        self.sink.stop();
        self.sink =
            Sink::try_new(&self.stream_handle).map_err(|e| DeezerError::Playback(e.to_string()))?;
        self.sink.set_volume(current_volume);
        self.sink.append(source);
        self.sink.play();

        info!(
            title = %track.title,
            artist = %track.artist,
            quality = quality.as_api_format(),
            "Now playing"
        );

        {
            let mut state = self.state.lock().unwrap();
            state.status = PlaybackStatus::Playing;
            state.current_track = Some(track.clone());
            state.duration_secs = track.duration_secs();
            state.position_secs = 0;
            state.quality = quality;
        }

        Ok(())
    }

    pub fn pause(&self) {
        self.sink.pause();
        let mut state = self.state.lock().unwrap();
        state.status = PlaybackStatus::Paused;
        debug!("Paused");
    }

    pub fn resume(&self) {
        self.sink.play();
        let mut state = self.state.lock().unwrap();
        state.status = PlaybackStatus::Playing;
        debug!("Resumed");
    }

    pub fn toggle_pause(&self) {
        let status = self.state.lock().unwrap().status;
        match status {
            PlaybackStatus::Playing => self.pause(),
            PlaybackStatus::Paused => self.resume(),
            _ => {}
        }
    }

    pub fn stop(&mut self) {
        self.sink.stop();
        // Recreate sink for future use
        if let Ok(new_sink) = Sink::try_new(&self.stream_handle) {
            self.sink = new_sink;
        }
        let mut state = self.state.lock().unwrap();
        state.status = PlaybackStatus::Stopped;
        state.current_track = None;
        state.position_secs = 0;
        state.duration_secs = 0;
        debug!("Stopped");
    }

    pub fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.sink.set_volume(volume);
        self.state.lock().unwrap().volume = volume;
    }

    pub fn volume(&self) -> f32 {
        self.state.lock().unwrap().volume
    }

    pub fn try_seek(&self, pos: Duration) -> Result<(), DeezerError> {
        self.sink
            .try_seek(pos)
            .map_err(|e| DeezerError::Playback(format!("Seek failed: {e}")))
    }

    pub fn is_finished(&self) -> bool {
        self.sink.empty()
    }
}
