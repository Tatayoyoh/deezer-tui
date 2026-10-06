//! 10-band graphic equalizer applied as a rodio `Source` wrapper.
//!
//! Gains live in an [`EqHandle`] shared (lock-free) between the daemon and the
//! audio thread, so a change is heard immediately without recreating the sink.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use biquad::{Biquad, Coefficients, DirectForm1, Hertz, Type};
use rodio::source::SeekError;
use rodio::Source;
use serde::{Deserialize, Serialize};

/// Number of equalizer bands.
pub const EQ_BAND_COUNT: usize = 10;

/// Center frequencies (Hz) of the bands, ISO octave spacing.
pub const EQ_FREQUENCIES: [f32; EQ_BAND_COUNT] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Gain range of a band, in dB (symmetric: `-EQ_MAX_GAIN_DB..=EQ_MAX_GAIN_DB`).
pub const EQ_MAX_GAIN_DB: f32 = 12.0;

/// Q of the peaking bands: roughly one octave wide.
const PEAK_Q: f32 = std::f32::consts::SQRT_2;
/// Q of the low/high shelves at the ends of the range.
const SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Gains closer to 0 dB than this are treated as flat and skipped.
const FLAT_EPSILON_DB: f32 = 0.05;
/// Peak ceiling of the limiter (linear, full scale = 1.0); about -0.4 dBFS.
const LIMIT_CEILING: f32 = 0.95;
/// Time for the limiter gain to recover after a peak.
const LIMIT_RELEASE_SECS: f32 = 0.15;

/// Built-in equalizer presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EqPreset {
    #[default]
    Flat,
    BassBoost,
    BassReducer,
    TrebleBoost,
    Vocal,
    Rock,
    Pop,
    Electronic,
    Classical,
    /// User-tuned gains.
    Custom,
}

impl EqPreset {
    /// Presets cycled through in the UI (`Custom` is reached by editing a band).
    pub const ALL: &[EqPreset] = &[
        EqPreset::Flat,
        EqPreset::BassBoost,
        EqPreset::BassReducer,
        EqPreset::TrebleBoost,
        EqPreset::Vocal,
        EqPreset::Rock,
        EqPreset::Pop,
        EqPreset::Electronic,
        EqPreset::Classical,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EqPreset::Flat => "Flat",
            EqPreset::BassBoost => "Bass Boost",
            EqPreset::BassReducer => "Bass Reducer",
            EqPreset::TrebleBoost => "Treble Boost",
            EqPreset::Vocal => "Vocal",
            EqPreset::Rock => "Rock",
            EqPreset::Pop => "Pop",
            EqPreset::Electronic => "Electronic",
            EqPreset::Classical => "Classical",
            EqPreset::Custom => "Custom",
        }
    }

    /// Band gains in dB, or `None` for `Custom`.
    pub fn gains(self) -> Option<[f32; EQ_BAND_COUNT]> {
        Some(match self {
            EqPreset::Flat => [0.0; EQ_BAND_COUNT],
            EqPreset::BassBoost => [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            EqPreset::BassReducer => [-6.0, -5.0, -4.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            EqPreset::TrebleBoost => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0, 5.0, 6.0],
            EqPreset::Vocal => [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, 0.0, -1.0],
            EqPreset::Rock => [5.0, 4.0, 2.0, -1.0, -2.0, -1.0, 1.0, 3.0, 4.0, 5.0],
            EqPreset::Pop => [-1.0, 1.0, 3.0, 4.0, 3.0, 0.0, -1.0, -1.0, 0.0, 1.0],
            EqPreset::Electronic => [5.0, 4.0, 1.0, 0.0, -2.0, 1.0, 0.0, 1.0, 4.0, 5.0],
            EqPreset::Classical => [4.0, 3.0, 2.0, 1.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0],
            EqPreset::Custom => return None,
        })
    }
}

/// Persisted equalizer configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub preset: EqPreset,
    #[serde(default)]
    pub gains: [f32; EQ_BAND_COUNT],
    /// Global gain in dB applied on top of the bands (independent of presets).
    #[serde(default)]
    pub preamp: f32,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: EqPreset::Flat,
            gains: [0.0; EQ_BAND_COUNT],
            preamp: 0.0,
        }
    }
}

impl EqSettings {
    /// Switch to a preset (no-op gains for `Custom`).
    pub fn apply_preset(&mut self, preset: EqPreset) {
        if let Some(gains) = preset.gains() {
            self.gains = gains;
        }
        self.preset = preset;
    }

    /// Change one band's gain (clamped); the preset becomes `Custom`.
    pub fn set_gain(&mut self, band: usize, gain_db: f32) {
        if let Some(g) = self.gains.get_mut(band) {
            *g = gain_db.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB);
            self.preset = EqPreset::Custom;
        }
    }

    /// Change the global gain (clamped). Presets are left untouched.
    pub fn set_preamp(&mut self, gain_db: f32) {
        self.preamp = gain_db.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB);
    }
}

/// Lock-free equalizer state shared with the audio thread.
#[derive(Debug)]
struct EqShared {
    enabled: AtomicBool,
    /// Gains in dB, stored as `f32::to_bits`.
    gains: [AtomicU32; EQ_BAND_COUNT],
    /// Global gain in dB, stored as `f32::to_bits`.
    preamp: AtomicU32,
    /// Bumped on every change so the audio thread knows to recompute coefficients.
    version: AtomicU32,
}

/// Cheap-to-clone handle to the live equalizer parameters.
#[derive(Debug, Clone)]
pub struct EqHandle(Arc<EqShared>);

impl Default for EqHandle {
    fn default() -> Self {
        Self::new(&EqSettings::default())
    }
}

impl EqHandle {
    pub fn new(settings: &EqSettings) -> Self {
        let handle = Self(Arc::new(EqShared {
            enabled: AtomicBool::new(false),
            gains: std::array::from_fn(|_| AtomicU32::new(0f32.to_bits())),
            preamp: AtomicU32::new(0f32.to_bits()),
            version: AtomicU32::new(0),
        }));
        handle.apply(settings);
        handle
    }

    /// Push new settings to the audio thread.
    pub fn apply(&self, settings: &EqSettings) {
        let s = &self.0;
        s.enabled.store(settings.enabled, Ordering::Relaxed);
        for (slot, gain) in s.gains.iter().zip(settings.gains) {
            let gain = gain.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB);
            slot.store(gain.to_bits(), Ordering::Relaxed);
        }
        let preamp = settings.preamp.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB);
        s.preamp.store(preamp.to_bits(), Ordering::Relaxed);
        s.version.fetch_add(1, Ordering::Release);
    }

    fn version(&self) -> u32 {
        self.0.version.load(Ordering::Acquire)
    }

    fn enabled(&self) -> bool {
        self.0.enabled.load(Ordering::Relaxed)
    }

    fn gains(&self) -> [f32; EQ_BAND_COUNT] {
        std::array::from_fn(|i| f32::from_bits(self.0.gains[i].load(Ordering::Relaxed)))
    }

    fn preamp(&self) -> f32 {
        f32::from_bits(self.0.preamp.load(Ordering::Relaxed))
    }
}

/// `Source` adapter running the decoded samples through the equalizer.
pub struct Equalizer<S> {
    inner: S,
    handle: EqHandle,
    /// Version of the handle the current filters were built from.
    seen_version: u32,
    /// One slot per band: `Some` (a biquad per channel) when the band is active.
    bands: [Option<Vec<DirectForm1<f32>>>; EQ_BAND_COUNT],
    /// False when disabled, or all bands and the global gain are flat:
    /// samples pass through untouched.
    active: bool,
    /// Linear input gain from the user's global gain.
    preamp: f32,
    /// Current limiter gain (1.0 = no reduction), shared by all channels so
    /// the stereo image doesn't shift.
    limiter_gain: f32,
    /// Per-sample smoothing factor of the limiter release.
    limiter_release: f32,
    channels: usize,
    sample_rate: u32,
    /// Channel of the next sample.
    channel: usize,
}

impl<S> Equalizer<S>
where
    S: Source<Item = i16>,
{
    pub fn new(inner: S, handle: EqHandle) -> Self {
        let channels = usize::from(inner.channels().max(1));
        let sample_rate = inner.sample_rate();
        let mut eq = Self {
            inner,
            seen_version: handle.version(),
            handle,
            bands: Default::default(),
            active: false,
            preamp: 1.0,
            limiter_gain: 1.0,
            // Samples are interleaved, so one second is `sample_rate * channels`.
            limiter_release: 1.0
                - (-1.0 / (LIMIT_RELEASE_SECS * sample_rate.max(1) as f32 * channels as f32)).exp(),
            channels,
            sample_rate,
            channel: 0,
        };
        eq.rebuild();
        eq
    }

    /// Recompute filter coefficients from the handle. Filters of bands that stay
    /// active keep their state, so live gain tweaks don't click.
    fn rebuild(&mut self) {
        self.seen_version = self.handle.version();
        let enabled = self.handle.enabled();
        let gains = self.handle.gains();
        let fs = self.sample_rate as f32;

        for (i, slot) in self.bands.iter_mut().enumerate() {
            let gain = gains[i];
            let coeffs = (enabled && gain.abs() >= FLAT_EPSILON_DB)
                .then(|| band_coefficients(i, gain, fs))
                .flatten();
            let Some(coeffs) = coeffs else {
                *slot = None;
                continue;
            };
            match slot {
                Some(filters) => filters
                    .iter_mut()
                    .for_each(|f| f.update_coefficients(coeffs)),
                None => *slot = Some(vec![DirectForm1::<f32>::new(coeffs); self.channels]),
            }
        }

        let user_preamp = if enabled { self.handle.preamp() } else { 0.0 };
        self.active =
            self.bands.iter().any(Option::is_some) || user_preamp.abs() >= FLAT_EPSILON_DB;
        // No automatic headroom: attenuating by the largest boost made every
        // non-boosted frequency quieter (Bass Boost sounded softer overall).
        // Peaks pushed over full scale are caught by the limiter in `next`.
        self.preamp = 10f32.powf(user_preamp / 20.0);
    }

    fn reset_filters(&mut self) {
        for filters in self.bands.iter_mut().flatten() {
            filters.iter_mut().for_each(Biquad::reset_state);
        }
        self.channel = 0;
        self.limiter_gain = 1.0;
    }

    /// Peak limiter: instant attack (the gain drops so this sample lands on
    /// the ceiling), smooth release. Transparent while nothing exceeds it.
    #[inline]
    fn limit(&mut self, x: f32) -> f32 {
        let peak = x.abs();
        let needed = if peak > LIMIT_CEILING {
            LIMIT_CEILING / peak
        } else {
            1.0
        };
        if needed < self.limiter_gain {
            self.limiter_gain = needed;
        } else {
            self.limiter_gain += (needed - self.limiter_gain) * self.limiter_release;
        }
        x * self.limiter_gain
    }
}

/// Coefficients for band `index` at `gain` dB: shelves at both ends, peaking in
/// between. `None` when the band is above Nyquist for this sample rate.
fn band_coefficients(index: usize, gain: f32, fs: f32) -> Option<Coefficients<f32>> {
    let freq = EQ_FREQUENCIES[index];
    if freq >= fs * 0.45 {
        return None;
    }
    let (kind, q) = match index {
        0 => (Type::LowShelf(gain), SHELF_Q),
        i if i == EQ_BAND_COUNT - 1 => (Type::HighShelf(gain), SHELF_Q),
        _ => (Type::PeakingEQ(gain), PEAK_Q),
    };
    let fs = Hertz::<f32>::from_hz(fs).ok()?;
    let f0 = Hertz::<f32>::from_hz(freq).ok()?;
    Coefficients::<f32>::from_params(kind, fs, f0, q).ok()
}

impl<S> Iterator for Equalizer<S>
where
    S: Source<Item = i16>,
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        let sample = self.inner.next()?;
        // Apply new settings only on a frame boundary so channels stay in step.
        if self.channel == 0 && self.handle.version() != self.seen_version {
            self.rebuild();
        }
        let channel = self.channel;
        self.channel = (channel + 1) % self.channels;
        if !self.active {
            return Some(sample);
        }

        let mut x = f32::from(sample) / 32768.0 * self.preamp;
        for filters in self.bands.iter_mut().flatten() {
            x = filters[channel].run(x);
        }
        let y = self.limit(x);
        Some((y * 32768.0).clamp(-32768.0, 32767.0) as i16)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Source for Equalizer<S>
where
    S: Source<Item = i16>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)?;
        // Stale filter memory from before the jump would produce a click.
        self.reset_filters();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    fn sine(freq: f32, fs: u32, secs: f32) -> Vec<i16> {
        let n = (fs as f32 * secs) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / fs as f32;
                ((2.0 * std::f32::consts::PI * freq * t).sin() * 8000.0) as i16
            })
            .collect()
    }

    fn rms(samples: &[i16]) -> f32 {
        // Skip the filter warm-up.
        let tail = &samples[samples.len() / 2..];
        let sum: f32 = tail.iter().map(|&s| f32::from(s).powi(2)).sum();
        (sum / tail.len() as f32).sqrt()
    }

    fn run(settings: &EqSettings, input: Vec<i16>) -> Vec<i16> {
        let src = SamplesBuffer::new(1, 44_100, input);
        Equalizer::new(src, EqHandle::new(settings)).collect()
    }

    #[test]
    fn disabled_is_bit_exact_passthrough() {
        let input = sine(1000.0, 44_100, 0.1);
        let mut settings = EqSettings::default();
        settings.apply_preset(EqPreset::Rock);
        assert_eq!(run(&settings, input.clone()), input);
    }

    #[test]
    fn cut_attenuates_band() {
        let input = sine(1000.0, 44_100, 0.2);
        let mut settings = EqSettings {
            enabled: true,
            ..Default::default()
        };
        settings.set_gain(5, -12.0); // 1 kHz band
        let ratio = rms(&run(&settings, input.clone())) / rms(&input);
        assert!(ratio < 0.4, "1 kHz should be cut, ratio = {ratio}");
    }

    #[test]
    fn boost_does_not_clip() {
        // Near full-scale bass, boosted +6 dB: the limiter must hold peaks at
        // the ceiling instead of flat-topping them at i16::MAX.
        let input: Vec<i16> = sine(62.0, 44_100, 0.5)
            .into_iter()
            .map(|s| s.saturating_mul(4))
            .collect();
        let mut settings = EqSettings {
            enabled: true,
            ..Default::default()
        };
        settings.apply_preset(EqPreset::BassBoost);
        let out = run(&settings, input);
        let ceiling = (LIMIT_CEILING * 32768.0) as i16 + 1;
        assert!(out
            .iter()
            .all(|&s| s.unsigned_abs() <= ceiling.unsigned_abs()));
    }

    #[test]
    fn boost_keeps_other_frequencies_level() {
        // Bass Boost must not make the mids quieter.
        let input = sine(1000.0, 44_100, 0.2);
        let mut settings = EqSettings {
            enabled: true,
            ..Default::default()
        };
        settings.apply_preset(EqPreset::BassBoost);
        let ratio = rms(&run(&settings, input.clone())) / rms(&input);
        assert!(
            (ratio - 1.0).abs() < 0.1,
            "1 kHz level changed, ratio = {ratio}"
        );
    }

    #[test]
    fn global_gain_scales_flat_signal() {
        let input = sine(1000.0, 44_100, 0.2);
        let mut settings = EqSettings {
            enabled: true,
            ..Default::default()
        };
        settings.set_preamp(-6.0);
        let ratio = rms(&run(&settings, input.clone())) / rms(&input);
        assert!((ratio - 0.5).abs() < 0.02, "-6 dB ≈ x0.5, ratio = {ratio}");
        // Ignored while the EQ is off.
        settings.enabled = false;
        assert_eq!(run(&settings, input.clone()), input);
    }

    #[test]
    fn live_update_is_picked_up() {
        let handle = EqHandle::default();
        let src = SamplesBuffer::new(1, 44_100, sine(1000.0, 44_100, 0.4));
        let mut eq = Equalizer::new(src, handle.clone());
        let before: Vec<i16> = eq.by_ref().take(8820).collect();
        let mut settings = EqSettings {
            enabled: true,
            ..Default::default()
        };
        settings.set_gain(5, -12.0);
        handle.apply(&settings);
        let after: Vec<i16> = eq.collect();
        assert!(rms(&after) < rms(&before) * 0.4);
    }
}
