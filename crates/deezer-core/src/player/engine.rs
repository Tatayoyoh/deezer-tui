use std::io::Cursor;
use std::sync::{Arc, Mutex};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use tracing::{debug, info};

use crate::api::models::{AudioQuality, DeezerError, TrackData};
use crate::player::state::{PlaybackStatus, PlayerState};

pub struct PlayerEngine {
    state: Arc<Mutex<PlayerState>>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Sink,
}

impl PlayerEngine {
    pub fn new(_master_key: [u8; 16]) -> Result<Self, DeezerError> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| DeezerError::Playback(e.to_string()))?;

        let sink =
            Sink::try_new(&stream_handle).map_err(|e| DeezerError::Playback(e.to_string()))?;

        Ok(Self {
            state: Arc::new(Mutex::new(PlayerState::default())),
            _stream: stream,
            stream_handle,
            sink,
        })
    }

    pub fn state(&self) -> Arc<Mutex<PlayerState>> {
        Arc::clone(&self.state)
    }

    /// Play pre-fetched and decrypted audio data.
    /// Called on the main thread with audio bytes from a background fetch.
    pub fn play_decoded(
        &mut self,
        audio_data: Vec<u8>,
        track: &TrackData,
        quality: AudioQuality,
    ) -> Result<(), DeezerError> {
        let cursor = Cursor::new(audio_data);
        let source = Decoder::new(cursor)
            .map_err(|e| DeezerError::Playback(format!("Failed to decode audio: {e}")))?;

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

    pub fn is_finished(&self) -> bool {
        self.sink.empty()
    }
}
