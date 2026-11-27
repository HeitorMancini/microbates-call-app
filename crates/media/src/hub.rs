//! [`MediaHub`]: the meeting point between capture devices, the transport and the UI.
//!
//! The core traits are deliberately independent (`AudioSource`, `ScreenSource`,
//! `Transport`), so raw media flows through this shared hub instead:
//!
//! * microphone capture -> [`MediaHub::push_mic`] -> transport (LiveKit audio source)
//! * screen capture -> [`MediaHub::push_screen_bgra`] -> transport (LiveKit video source)
//! * remote audio -> [`MediaHub::push_remote_audio`] -> playback mixer -> speakers
//! * remote/local video -> [`MediaHub::subscribe_previews`] -> UI (`room://screen-frame`)

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use livekit::webrtc::prelude::{I420Buffer, VideoBuffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use tokio::sync::{broadcast, mpsc};

use crate::gate::{SENSITIVITY_OPEN, SILENCE_DBFS};
use crate::video::{bgra_to_i420, fit_within, i420_to_rgba_preview, scale_i420};

/// Sample rate of every PCM buffer exchanged through the hub.
pub const SAMPLE_RATE: u32 = 48_000;
/// Samples (mono) in one 10 ms chunk, the unit pushed by microphones.
pub const SAMPLES_PER_10MS: usize = (SAMPLE_RATE / 100) as usize;

/// Remote audio buffered per participant before old samples are dropped.
const MAX_BUFFERED_SAMPLES: usize = SAMPLES_PER_10MS * 30;
/// Samples a participant must accumulate before playback starts (jitter cushion).
const PRIME_SAMPLES: usize = SAMPLES_PER_10MS * 4;

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Highest volume (percent) accepted for the microphone and for each participant.
pub const MAX_VOLUME_PERCENT: u32 = 200;

/// Scales `samples` by `percent` (100 = unchanged), saturating at the i16 range.
pub(crate) fn apply_gain(samples: &mut [i16], percent: u32) {
    if percent == 100 {
        return;
    }
    for s in samples {
        let v = i64::from(*s) * i64::from(percent) / 100;
        *s = v.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    }
}

const SCREEN_AUDIO_SUFFIX: &str = "#screen";

/// Mixer key of the screen-share audio of `identity`, kept apart from their voice.
pub(crate) fn screen_audio_key(identity: &str) -> String {
    format!("{identity}{SCREEN_AUDIO_SUFFIX}")
}

/// Limits applied to frames forwarded to the UI (`room://screen-frame`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewConfig {
    pub max_fps: u32,
    pub max_width: u32,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            max_fps: 15,
            max_width: 1280,
        }
    }
}

/// A decoded video frame ready for the UI, in tightly packed RGBA.
#[derive(Debug, Clone)]
pub struct PreviewFrame {
    pub participant_id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

/// Counters useful for tests and diagnostics.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MediaStats {
    pub mic_chunks_sent: u64,
    pub screen_frames_sent: u64,
    pub remote_audio_samples: u64,
    pub remote_video_frames: u64,
    pub played_samples: u64,
}

#[derive(Default)]
struct Counters {
    mic_chunks_sent: AtomicU64,
    screen_frames_sent: AtomicU64,
    remote_audio_samples: AtomicU64,
    remote_video_frames: AtomicU64,
    played_samples: AtomicU64,
}

#[derive(Default)]
struct SourceBuffer {
    samples: VecDeque<i16>,
    primed: bool,
}

fn owner_gain(gains: &HashMap<String, u32>, id: &str) -> u32 {
    let owner = id.strip_suffix(SCREEN_AUDIO_SUFFIX).unwrap_or(id);
    gains.get(owner).copied().unwrap_or(100)
}

#[derive(Default)]
struct Mixer {
    sources: HashMap<String, SourceBuffer>,
    /// Volume (percent) chosen by the local user per participant; absent means 100.
    gains: HashMap<String, u32>,
}

impl Mixer {
    fn set_gain(&mut self, id: &str, percent: u32) {
        self.gains
            .insert(id.to_owned(), percent.min(MAX_VOLUME_PERCENT));
    }

    /// Gain of the person owning mixer key `id` (their screen audio follows their voice).
    #[cfg(test)]
    fn gain(&self, id: &str) -> u32 {
        owner_gain(&self.gains, id)
    }

    fn push(&mut self, id: &str, samples: &[i16]) {
        let buf = self.sources.entry(id.to_owned()).or_default();
        buf.samples.extend(samples.iter().copied());
        let overflow = buf.samples.len().saturating_sub(MAX_BUFFERED_SAMPLES);
        buf.samples.drain(..overflow);
    }

    fn mix(&mut self, out: &mut [i16]) {
        out.fill(0);
        let gains = &self.gains;
        for (id, buf) in &mut self.sources {
            if !buf.primed {
                if buf.samples.len() < PRIME_SAMPLES {
                    continue;
                }
                buf.primed = true;
            }
            let gain = owner_gain(gains, id);
            let n = out.len().min(buf.samples.len());
            let mut chunk: Vec<i16> = buf.samples.drain(..n).collect();
            apply_gain(&mut chunk, gain);
            for (o, s) in out.iter_mut().zip(chunk) {
                *o = o.saturating_add(s);
            }
            if buf.samples.is_empty() {
                buf.primed = false;
            }
        }
    }
}

/// Receives screen frames for an encoder outside libwebrtc (hardware encoder), with the
/// capture timestamp in microseconds.
pub(crate) type ScreenFeed = Arc<dyn Fn(&I420Buffer, i64) + Send + Sync>;

/// Where screen frames go: libwebrtc's own encoder or an external one.
pub(crate) enum ScreenOutput {
    Source(NativeVideoSource),
    Feed(ScreenFeed),
}

struct ScreenSink {
    output: ScreenOutput,
    max_width: u32,
    max_height: u32,
    /// Last frame handed to the encoder and when, for [`MediaHub::repeat_screen_if_idle`].
    last: Option<(I420Buffer, Instant)>,
}

struct PreviewThrottle {
    last: HashMap<String, Instant>,
}

struct Inner {
    started: Instant,
    mic: Mutex<Option<mpsc::Sender<Vec<i16>>>>,
    screen_audio: Mutex<Option<mpsc::Sender<Vec<i16>>>>,
    screen: Mutex<Option<ScreenSink>>,
    mixer: Mutex<Mixer>,
    previews: broadcast::Sender<PreviewFrame>,
    preview_config: PreviewConfig,
    throttle: Mutex<PreviewThrottle>,
    local_identity: Mutex<Option<String>>,
    mic_volume: AtomicU32,
    noise_suppression: AtomicBool,
    mic_sensitivity: AtomicU32,
    /// Current microphone level in dBFS, as `f32` bits (see [`MediaHub::mic_level_dbfs`]).
    mic_level: AtomicU32,
    counters: Counters,
}

/// Cheaply cloneable handle shared by all media components of one app instance.
#[derive(Clone)]
pub struct MediaHub {
    inner: Arc<Inner>,
}

impl Default for MediaHub {
    fn default() -> Self {
        Self::new(PreviewConfig::default())
    }
}

impl std::fmt::Debug for MediaHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaHub")
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl MediaHub {
    pub fn new(preview_config: PreviewConfig) -> Self {
        let (previews, _) = broadcast::channel(4);
        Self {
            inner: Arc::new(Inner {
                started: Instant::now(),
                mic: Mutex::new(None),
                screen_audio: Mutex::new(None),
                screen: Mutex::new(None),
                mixer: Mutex::new(Mixer::default()),
                previews,
                preview_config,
                throttle: Mutex::new(PreviewThrottle {
                    last: HashMap::new(),
                }),
                local_identity: Mutex::new(None),
                mic_volume: AtomicU32::new(100),
                noise_suppression: AtomicBool::new(true),
                mic_sensitivity: AtomicU32::new(SENSITIVITY_OPEN),
                mic_level: AtomicU32::new(SILENCE_DBFS.to_bits()),
                counters: Counters::default(),
            }),
        }
    }

    pub fn stats(&self) -> MediaStats {
        let c = &self.inner.counters;
        MediaStats {
            mic_chunks_sent: c.mic_chunks_sent.load(Ordering::Relaxed),
            screen_frames_sent: c.screen_frames_sent.load(Ordering::Relaxed),
            remote_audio_samples: c.remote_audio_samples.load(Ordering::Relaxed),
            remote_video_frames: c.remote_video_frames.load(Ordering::Relaxed),
            played_samples: c.played_samples.load(Ordering::Relaxed),
        }
    }

    pub fn preview_config(&self) -> PreviewConfig {
        self.inner.preview_config
    }

    /// Frames (remote shares and the local share) throttled for display in the UI.
    pub fn subscribe_previews(&self) -> broadcast::Receiver<PreviewFrame> {
        self.inner.previews.subscribe()
    }

    // ---- microphone -> transport ------------------------------------------------------

    pub(crate) fn set_mic_sink(&self, sink: Option<mpsc::Sender<Vec<i16>>>) {
        *lock(&self.inner.mic) = sink;
    }

    /// Sets the microphone gain in percent (clamped to `0..=MAX_VOLUME_PERCENT`).
    pub fn set_mic_volume(&self, percent: u32) {
        self.inner
            .mic_volume
            .store(percent.min(MAX_VOLUME_PERCENT), Ordering::Relaxed);
    }

    pub fn mic_volume(&self) -> u32 {
        self.inner.mic_volume.load(Ordering::Relaxed)
    }

    /// Turns RNNoise noise suppression on the microphone on or off (on by default).
    pub fn set_noise_suppression(&self, enabled: bool) {
        self.inner
            .noise_suppression
            .store(enabled, Ordering::Relaxed);
    }

    pub fn noise_suppression(&self) -> bool {
        self.inner.noise_suppression.load(Ordering::Relaxed)
    }

    /// Sets the microphone sensitivity (clamped to `0..=100`); 100, the default, never cuts.
    pub fn set_mic_sensitivity(&self, percent: u32) {
        self.inner
            .mic_sensitivity
            .store(percent.min(SENSITIVITY_OPEN), Ordering::Relaxed);
    }

    pub fn mic_sensitivity(&self) -> u32 {
        self.inner.mic_sensitivity.load(Ordering::Relaxed)
    }

    /// Microphone level in dBFS before the sensitivity gate, with a slow fall so polling
    /// sees peaks. `-100` when there is no signal or the microphone is muted.
    pub fn mic_level_dbfs(&self) -> f32 {
        f32::from_bits(self.inner.mic_level.load(Ordering::Relaxed))
    }

    pub(crate) fn set_mic_level_dbfs(&self, db: f32) {
        self.inner.mic_level.store(db.to_bits(), Ordering::Relaxed);
    }

    /// Queues 48 kHz mono PCM for the published microphone track. Returns `false` when
    /// no transport is listening (or it is lagging and the chunk was dropped).
    pub fn push_mic(&self, samples: &[i16]) -> bool {
        let guard = lock(&self.inner.mic);
        let Some(tx) = guard.as_ref() else {
            return false;
        };
        let mut chunk = samples.to_vec();
        apply_gain(&mut chunk, self.mic_volume());
        let sent = tx.try_send(chunk).is_ok();
        if sent {
            self.inner
                .counters
                .mic_chunks_sent
                .fetch_add(1, Ordering::Relaxed);
        }
        sent
    }

    // ---- screen audio -> transport ----------------------------------------------------

    pub(crate) fn set_screen_audio_sink(&self, sink: Option<mpsc::Sender<Vec<i16>>>) {
        *lock(&self.inner.screen_audio) = sink;
    }

    /// Queues 48 kHz mono PCM for the published screen-share audio track. Returns `false`
    /// when nothing is listening or the chunk was dropped because the transport lags.
    pub fn push_screen_audio(&self, samples: &[i16]) -> bool {
        let guard = lock(&self.inner.screen_audio);
        guard
            .as_ref()
            .is_some_and(|tx| tx.try_send(samples.to_vec()).is_ok())
    }

    // ---- screen -> transport ----------------------------------------------------------

    pub(crate) fn set_screen_sink(&self, sink: Option<(ScreenOutput, u32, u32)>) {
        *lock(&self.inner.screen) = sink.map(|(output, max_width, max_height)| ScreenSink {
            output,
            max_width,
            max_height,
            last: None,
        });
    }

    pub(crate) fn set_local_identity(&self, identity: Option<String>) {
        *lock(&self.inner.local_identity) = identity;
    }

    /// Whether a published screen track is currently waiting for frames.
    pub fn screen_sink_active(&self) -> bool {
        lock(&self.inner.screen).is_some()
    }

    /// Feeds one captured BGRA frame (`stride` bytes per row) to the published screen
    /// track, downscaling it to fit the active preset. Returns `false` when not sharing.
    pub fn push_screen_bgra(&self, width: u32, height: u32, stride: u32, data: &[u8]) -> bool {
        let local = lock(&self.inner.local_identity).clone();
        let mut guard = lock(&self.inner.screen);
        let Some(sink) = guard.as_mut() else {
            return false;
        };
        let mut i420 = bgra_to_i420(width, height, stride, data);
        let (w, h) = fit_within(width, height, sink.max_width, sink.max_height);
        if (w, h) != (i420.width(), i420.height()) {
            i420 = scale_i420(&mut i420, w, h);
        }
        self.send_screen(sink, i420);
        self.inner
            .counters
            .screen_frames_sent
            .fetch_add(1, Ordering::Relaxed);
        if let (Some(id), Some((buffer, _))) = (local, sink.last.as_mut()) {
            self.maybe_preview(&id, buffer);
        }
        true
    }

    /// Re-sends the last screen frame once capture has been idle for `idle`.
    ///
    /// WGC only emits frames when the screen changes, and the encoder can only answer a
    /// keyframe request (PLI from a viewer that just joined or lost packets) on its next
    /// input frame: without repeats a static screen leaves viewers waiting for an image.
    pub fn repeat_screen_if_idle(&self, idle: Duration) -> bool {
        let mut guard = lock(&self.inner.screen);
        let Some(sink) = guard.as_mut() else {
            return false;
        };
        let Some((last, at)) = sink.last.as_mut() else {
            return false;
        };
        if at.elapsed() < idle {
            return false;
        }
        let (w, h) = (last.width(), last.height());
        let copy = scale_i420(last, w, h);
        self.send_screen(sink, copy);
        true
    }

    fn send_screen(&self, sink: &mut ScreenSink, buffer: I420Buffer) {
        let mut frame = VideoFrame::new(VideoRotation::VideoRotation0, buffer);
        frame.timestamp_us = frame_timestamp(&self.inner.started);
        match &sink.output {
            ScreenOutput::Source(source) => {
                source.capture_frame(&frame);
            }
            ScreenOutput::Feed(feed) => feed(&frame.buffer, frame.timestamp_us),
        }
        sink.last = Some((frame.buffer, Instant::now()));
    }

    // ---- remote media -----------------------------------------------------------------

    /// Adds decoded 48 kHz mono PCM from a remote participant to the playback mixer.
    pub fn push_remote_audio(&self, participant_id: &str, samples: &[i16]) {
        self.inner
            .counters
            .remote_audio_samples
            .fetch_add(samples.len() as u64, Ordering::Relaxed);
        lock(&self.inner.mixer).push(participant_id, samples);
    }

    /// Sets how loud `participant_id` (voice and screen audio) is for the local user, in
    /// percent (clamped to `0..=MAX_VOLUME_PERCENT`). Kept until the app restarts.
    pub fn set_participant_volume(&self, participant_id: &str, percent: u32) {
        lock(&self.inner.mixer).set_gain(participant_id, percent);
    }

    pub fn remove_remote(&self, participant_id: &str) {
        let mut mixer = lock(&self.inner.mixer);
        mixer.sources.remove(participant_id);
        mixer.sources.remove(&screen_audio_key(participant_id));
        drop(mixer);
        lock(&self.inner.throttle).last.remove(participant_id);
    }

    pub fn clear_remotes(&self) {
        lock(&self.inner.mixer).sources.clear();
        lock(&self.inner.throttle).last.clear();
    }

    /// Fills `out` (48 kHz mono) with the mix of all remote participants.
    pub fn mix_playback(&self, out: &mut [i16]) {
        lock(&self.inner.mixer).mix(out);
        self.inner
            .counters
            .played_samples
            .fetch_add(out.len() as u64, Ordering::Relaxed);
    }

    /// Records a decoded remote video frame (for stats).
    pub(crate) fn count_remote_video(&self) {
        self.inner
            .counters
            .remote_video_frames
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Whether a preview for `participant_id` should be produced right now.
    pub(crate) fn preview_due(&self, participant_id: &str) -> bool {
        if self.inner.previews.receiver_count() == 0 {
            return false;
        }
        let interval = Duration::from_secs(1) / self.inner.preview_config.max_fps.max(1);
        let now = Instant::now();
        let mut throttle = lock(&self.inner.throttle);
        match throttle.last.get(participant_id) {
            Some(last) if now.duration_since(*last) < interval => false,
            _ => {
                throttle.last.insert(participant_id.to_owned(), now);
                true
            }
        }
    }

    fn maybe_preview(&self, participant_id: &str, buffer: &mut I420Buffer) {
        if self.preview_due(participant_id) {
            self.emit_preview(participant_id, buffer);
        }
    }

    /// Converts `buffer` to an RGBA preview and broadcasts it (no throttling).
    pub(crate) fn emit_preview(&self, participant_id: &str, buffer: &mut I420Buffer) {
        let (width, height, rgba) =
            i420_to_rgba_preview(buffer, self.inner.preview_config.max_width);
        let _ = self.inner.previews.send(PreviewFrame {
            participant_id: participant_id.to_owned(),
            width,
            height,
            rgba: Arc::new(rgba),
        });
    }
}

fn frame_timestamp(started: &Instant) -> i64 {
    i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_waits_for_prime_then_sums_with_saturation() {
        let mut mixer = Mixer::default();
        let mut out = vec![0i16; SAMPLES_PER_10MS];
        mixer.push("a", &vec![1000; SAMPLES_PER_10MS]);
        mixer.mix(&mut out);
        assert!(out.iter().all(|&s| s == 0), "not primed yet");

        mixer.push("a", &vec![i16::MAX; PRIME_SAMPLES]);
        mixer.push("b", &vec![i16::MAX; PRIME_SAMPLES]);
        mixer.mix(&mut out);
        assert_eq!(out[0], i16::MAX);
    }

    #[test]
    fn participant_volume_scales_voice_and_screen_audio_and_survives_rejoin() {
        let mut mixer = Mixer::default();
        mixer.set_gain("a", 50);
        mixer.set_gain("b", 500);
        assert_eq!(mixer.gain("b"), MAX_VOLUME_PERCENT);
        assert_eq!(mixer.gain("nobody"), 100);
        for id in ["a", "a#screen", "b"] {
            mixer.push(id, &vec![1000; PRIME_SAMPLES]);
        }
        let mut out = vec![0i16; SAMPLES_PER_10MS];
        mixer.mix(&mut out);
        // 500 (a) + 500 (a#screen) + 2000 (b)
        assert_eq!(out[0], 3000);

        mixer.sources.clear();
        assert_eq!(
            mixer.gain("a"),
            50,
            "gain outlives the participant's buffers"
        );
        mixer.set_gain("a", 100);
        assert_eq!(mixer.gain("a"), 100);
    }

    #[test]
    fn mixer_bounds_buffered_audio() {
        let mut mixer = Mixer::default();
        mixer.push("a", &vec![1; MAX_BUFFERED_SAMPLES * 2]);
        assert_eq!(mixer.sources["a"].samples.len(), MAX_BUFFERED_SAMPLES);
    }

    #[tokio::test]
    async fn mic_chunks_flow_only_when_a_sink_is_installed() {
        let hub = MediaHub::default();
        assert!(!hub.push_mic(&[0; SAMPLES_PER_10MS]));
        let (tx, mut rx) = mpsc::channel(4);
        hub.set_mic_sink(Some(tx));
        assert!(hub.push_mic(&[7; SAMPLES_PER_10MS]));
        assert_eq!(rx.recv().await.unwrap()[0], 7);
        assert_eq!(hub.stats().mic_chunks_sent, 1);
    }

    #[tokio::test]
    async fn mic_volume_scales_chunks_with_saturation_and_clamps() {
        let hub = MediaHub::default();
        let (tx, mut rx) = mpsc::channel(8);
        hub.set_mic_sink(Some(tx));
        hub.push_mic(&[1000; 2]);
        assert_eq!(rx.recv().await.unwrap(), vec![1000; 2], "100% is untouched");

        hub.set_mic_volume(50);
        hub.push_mic(&[1000, -1000]);
        assert_eq!(rx.recv().await.unwrap(), vec![500, -500]);

        hub.set_mic_volume(500);
        assert_eq!(hub.mic_volume(), MAX_VOLUME_PERCENT);
        hub.push_mic(&[i16::MAX, 100]);
        assert_eq!(rx.recv().await.unwrap(), vec![i16::MAX, 200]);

        hub.set_mic_volume(0);
        hub.push_mic(&[1000]);
        assert_eq!(rx.recv().await.unwrap(), vec![0]);
    }

    #[test]
    fn mic_processing_settings_default_and_clamp() {
        let hub = MediaHub::default();
        assert!(
            hub.noise_suppression(),
            "noise suppression is on by default"
        );
        assert_eq!(hub.mic_sensitivity(), 100, "gate open by default");
        assert_eq!(hub.mic_level_dbfs(), -100.0);

        hub.set_noise_suppression(false);
        assert!(!hub.noise_suppression());
        hub.set_mic_sensitivity(40);
        assert_eq!(hub.mic_sensitivity(), 40);
        hub.set_mic_sensitivity(900);
        assert_eq!(hub.mic_sensitivity(), 100);
        hub.set_mic_level_dbfs(-33.5);
        assert_eq!(hub.mic_level_dbfs(), -33.5);
    }

    #[tokio::test]
    async fn screen_audio_flows_only_when_a_sink_is_installed() {
        let hub = MediaHub::default();
        assert!(!hub.push_screen_audio(&[1; SAMPLES_PER_10MS]));
        let (tx, mut rx) = mpsc::channel(4);
        hub.set_screen_audio_sink(Some(tx));
        assert!(hub.push_screen_audio(&[1; SAMPLES_PER_10MS]));
        assert_eq!(rx.recv().await.unwrap(), vec![1; SAMPLES_PER_10MS]);
        hub.set_screen_audio_sink(None);
        assert!(!hub.push_screen_audio(&[1; SAMPLES_PER_10MS]));
    }

    #[test]
    fn remove_remote_also_drops_its_screen_audio() {
        let hub = MediaHub::default();
        hub.push_remote_audio("a", &[1; SAMPLES_PER_10MS]);
        hub.push_remote_audio(&screen_audio_key("a"), &[1; SAMPLES_PER_10MS]);
        assert_eq!(screen_audio_key("a"), "a#screen");
        hub.remove_remote("a");
        assert!(lock(&hub.inner.mixer).sources.is_empty());
    }

    #[test]
    fn idle_screen_repeats_the_last_frame() {
        let hub = MediaHub::default();
        assert!(!hub.repeat_screen_if_idle(Duration::ZERO), "not sharing");
        let fed = Arc::new(Mutex::new(Vec::new()));
        let sink = fed.clone();
        let feed: ScreenFeed = Arc::new(move |frame: &I420Buffer, timestamp_us| {
            lock(&sink).push((frame.width(), frame.height(), timestamp_us));
        });
        hub.set_screen_sink(Some((ScreenOutput::Feed(feed), 64, 32)));
        assert!(!hub.repeat_screen_if_idle(Duration::ZERO), "no frame yet");
        assert!(hub.push_screen_bgra(64, 32, 64 * 4, &vec![0x80; 64 * 32 * 4]));
        assert!(!hub.repeat_screen_if_idle(Duration::from_secs(3600)));
        assert!(hub.repeat_screen_if_idle(Duration::ZERO));
        assert_eq!(
            hub.stats().screen_frames_sent,
            1,
            "repeats are not captures"
        );
        let fed = lock(&fed);
        assert_eq!(fed.len(), 2);
        assert!(fed.iter().all(|&(w, h, _)| (w, h) == (64, 32)));
        assert!(fed[1].2 >= fed[0].2, "repeats carry a fresh timestamp");
    }

    #[test]
    fn previews_are_throttled_per_participant() {
        let hub = MediaHub::new(PreviewConfig {
            max_fps: 1,
            max_width: 64,
        });
        assert!(!hub.preview_due("a"), "no subscribers, no work");
        let _rx = hub.subscribe_previews();
        assert!(hub.preview_due("a"));
        assert!(!hub.preview_due("a"));
        assert!(hub.preview_due("b"));
    }
}
