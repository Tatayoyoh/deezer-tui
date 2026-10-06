use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::api::models::{AudioQuality, DeezerError, TrackData};
use crate::api::DeezerClient;
use crate::decrypt::{self, BLOCK_SIZE};

/// Bytes buffered before playback starts (~2 s of FLAC, ~16 s of MP3 128).
const PREBUFFER_BYTES: usize = 256 * 1024;

/// Whole-body limit for a streamed download. The CDN client's 30 s timeout is
/// sized for full downloads of small files; a FLAC track on a slow link can
/// legitimately take longer while it is already playing.
const STREAM_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Create a shared HTTP client suitable for CDN downloads (no cookies needed).
pub fn new_cdn_client() -> Result<reqwest::Client, DeezerError> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| DeezerError::Http(e.to_string()))
}

/// Download and decrypt a full track, returning raw audio bytes (MP3 or FLAC).
pub async fn fetch_track(
    client: &DeezerClient,
    track: &TrackData,
    quality: AudioQuality,
    master_key: &[u8; 16],
) -> Result<(Vec<u8>, AudioQuality), DeezerError> {
    // Get streaming URL (with quality fallback)
    let (url, actual_quality) = client.get_stream_url(track, quality).await?;

    let data = download_and_decrypt(&url, &track.track_id, master_key, client.http()).await?;

    Ok((data, actual_quality))
}

/// Download encrypted audio from a CDN URL and decrypt it.
/// This function does NOT need a `DeezerClient` reference, so it can run
/// without holding the client lock.
/// Pass a shared `reqwest::Client` to reuse connections across downloads.
pub async fn download_and_decrypt(
    url: &str,
    track_id: &str,
    master_key: &[u8; 16],
    http: &reqwest::Client,
) -> Result<Vec<u8>, DeezerError> {
    info!(track_id = %track_id, "Downloading track");

    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(DeezerError::Http(format!(
            "CDN returned status {}",
            resp.status()
        )));
    }

    let mut data = resp
        .bytes()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?
        .to_vec();

    debug!(
        bytes = data.len(),
        track_id = %track_id,
        "Downloaded encrypted audio"
    );

    // Derive per-track key and decrypt
    let track_key = decrypt::derive_track_key(track_id, master_key);
    decrypt::decrypt_stream(&mut data, &track_key)?;

    debug!(track_id = %track_id, "Decrypted audio");

    Ok(data)
}

/// Download unencrypted audio, for podcast episodes served by the show's own
/// host rather than Deezer's CDN.
pub async fn download_direct(url: &str, http: &reqwest::Client) -> Result<Vec<u8>, DeezerError> {
    info!("Downloading direct stream");

    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(DeezerError::Http(format!(
            "Stream host returned status {}",
            resp.status()
        )));
    }

    let data = resp
        .bytes()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?
        .to_vec();

    debug!(bytes = data.len(), "Downloaded direct stream");

    Ok(data)
}

/// Decrypted audio shared between the download task (writer) and the decoder
/// (reader). Grows as blocks arrive; readers block until their bytes are there.
struct StreamBuffer {
    state: Mutex<StreamState>,
    grew: Condvar,
    /// Full file size from `Content-Length`, needed to answer `SeekFrom::End`.
    total_len: Option<u64>,
    /// Set when the reader is dropped (track skipped): the download stops.
    cancelled: AtomicBool,
}

#[derive(Default)]
struct StreamState {
    data: Vec<u8>,
    done: bool,
}

/// `Read + Seek` view over a track that is still downloading, so playback can
/// start after a short prebuffer instead of after the whole file.
pub struct StreamReader {
    buf: Arc<StreamBuffer>,
    pos: u64,
}

impl Read for StreamReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let mut st = self.buf.state.lock().unwrap();
        loop {
            let len = st.data.len() as u64;
            if self.pos < len {
                let start = self.pos as usize;
                let n = out.len().min(st.data.len() - start);
                out[..n].copy_from_slice(&st.data[start..start + n]);
                self.pos += n as u64;
                return Ok(n);
            }
            if st.done {
                // Download finished (or failed): end of track.
                return Ok(0);
            }
            st = self.buf.grew.wait(st).unwrap();
        }
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            SeekFrom::End(d) => {
                let total = match self.buf.total_len {
                    Some(t) => t,
                    None => {
                        // No Content-Length: the end is only known once downloaded.
                        let mut st = self.buf.state.lock().unwrap();
                        while !st.done {
                            st = self.buf.grew.wait(st).unwrap();
                        }
                        st.data.len() as u64
                    }
                };
                total.checked_add_signed(d)
            }
        };
        // Seeking past what has arrived is fine: the next read waits for it.
        self.pos = target.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek before start of stream")
        })?;
        Ok(self.pos)
    }
}

impl Drop for StreamReader {
    fn drop(&mut self) {
        self.buf.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Start downloading an encrypted track and return a reader as soon as
/// `PREBUFFER_BYTES` are decrypted. The rest keeps downloading in a background
/// task, decrypting each 2048-byte block as it arrives (Deezer's stripe cipher
/// restarts its CBC chain on every block, so blocks decrypt independently).
pub async fn stream_and_decrypt(
    url: &str,
    track_id: &str,
    master_key: &[u8; 16],
    http: &reqwest::Client,
) -> Result<StreamReader, DeezerError> {
    info!(track_id = %track_id, "Streaming track");

    let mut resp = http
        .get(url)
        .timeout(STREAM_TIMEOUT)
        .send()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(DeezerError::Http(format!(
            "CDN returned status {}",
            resp.status()
        )));
    }

    let buf = Arc::new(StreamBuffer {
        state: Mutex::new(StreamState::default()),
        grew: Condvar::new(),
        total_len: resp.content_length(),
        cancelled: AtomicBool::new(false),
    });
    let track_key = decrypt::derive_track_key(track_id, master_key);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<Result<(), DeezerError>>();

    let writer = Arc::clone(&buf);
    let track_id = track_id.to_string();
    tokio::spawn(async move {
        let mut ready_tx = Some(ready_tx);
        let mut pending: Vec<u8> = Vec::with_capacity(4 * BLOCK_SIZE);
        let mut next_block = 0usize;

        let result: Result<(), DeezerError> = loop {
            if writer.cancelled.load(Ordering::Relaxed) {
                debug!(track_id = %track_id, "Stream cancelled");
                break Ok(());
            }
            let chunk = match resp.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => {
                    // Trailing partial block is never encrypted.
                    let mut st = writer.state.lock().unwrap();
                    st.data.append(&mut pending);
                    break Ok(());
                }
                Err(e) => break Err(DeezerError::Http(e.to_string())),
            };
            pending.extend_from_slice(&chunk);

            let whole = pending.len() / BLOCK_SIZE * BLOCK_SIZE;
            if whole == 0 {
                continue;
            }
            let mut blocks: Vec<u8> = pending.drain(..whole).collect();
            let mut failed = None;
            for block in blocks.chunks_exact_mut(BLOCK_SIZE) {
                if let Err(e) = decrypt::decrypt_block(block, next_block, &track_key) {
                    failed = Some(e);
                    break;
                }
                next_block += 1;
            }
            if let Some(e) = failed {
                break Err(e);
            }

            let len = {
                let mut st = writer.state.lock().unwrap();
                st.data.extend_from_slice(&blocks);
                st.data.len()
            };
            writer.grew.notify_all();
            if len >= PREBUFFER_BYTES {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(Ok(()));
                }
            }
        };

        let total = {
            let mut st = writer.state.lock().unwrap();
            st.done = true;
            st.data.len()
        };
        writer.grew.notify_all();
        match &result {
            Ok(()) => debug!(track_id = %track_id, bytes = total, "Stream complete"),
            Err(e) => warn!(track_id = %track_id, bytes = total, err = %e, "Stream failed"),
        }
        if let Some(tx) = ready_tx.take() {
            // Short file, or failed before the prebuffer filled.
            let _ = tx.send(if total > 0 { Ok(()) } else { result });
        }
    });

    match ready_rx.await {
        Ok(Ok(())) => Ok(StreamReader { buf, pos: 0 }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(DeezerError::Http("stream task ended unexpectedly".into())),
    }
}
