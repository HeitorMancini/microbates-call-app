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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use livekit::webrtc::prelude::{I420Buffer, VideoBuffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use tokio::sync::{broadcast, mpsc};

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

#[derive(Default)]
struct Mixer {
    sources: HashMap<String, SourceBuffer>,
}

impl Mixer {
    fn push(&mut self, id: &str, samples: &[i16]) {
        let buf = self.sources.entry(id.to_owned()).or_default();
        buf.samples.extend(samples.iter().copied());
        let overflow = buf.samples.len().saturating_sub(MAX_BUFFERED_SAMPLES);
        buf.samples.drain(..overflow);
    }

    fn mix(&mut self, out: &mut [i16]) {
        out.fill(0);
        for buf in self.sources.values_mut() {
            if !buf.primed {
                if buf.samples.len() < PRIME_SAMPLES {
                    continue;
                }
                buf.primed = true;
            }
            let n = out.len().min(buf.samples.len());
            for (o, s) in out.iter_mut().zip(buf.samples.drain(..n)) {
                *o = o.saturating_add(s);
            }
            if buf.samples.is_empty() {
                buf.primed = false;
            }
        }
    }
}

struct ScreenSink {
    source: NativeVideoSource,
    max_width: u32,
    max_height: u32,
}

struct PreviewThrottle {
    last: HashMap<String, Instant>,
}

struct Inner {
    started: Instant,
    mic: Mutex<Option<mpsc::Sender<Vec<i16>>>>,
    screen: Mutex<Option<ScreenSink>>,
    mixer: Mutex<Mixer>,
    previews: broadcast::Sender<PreviewFrame>,
    preview_config: PreviewConfig,
    throttle: Mutex<PreviewThrottle>,
    local_identity: Mutex<Option<String>>,
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
                screen: Mutex::new(None),
                mixer: Mutex::new(Mixer::default()),
                previews,
                preview_config,
                throttle: Mutex::new(PreviewThrottle {
                    last: HashMap::new(),
                }),
                local_identity: Mutex::new(None),
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

    /// Queues 48 kHz mono PCM for the published microphone track. Returns `false` when
    /// no transport is listening (or it is lagging and the chunk was dropped).
    pub fn push_mic(&self, samples: &[i16]) -> bool {
        let guard = lock(&self.inner.mic);
        let Some(tx) = guard.as_ref() else {
            return false;
        };
        let sent = tx.try_send(samples.to_vec()).is_ok();
        if sent {
            self.inner
                .counters
                .mic_chunks_sent
                .fetch_add(1, Ordering::Relaxed);
        }
        sent
    }

    // ---- screen -> transport ----------------------------------------------------------

    pub(crate) fn set_screen_sink(&self, source: Option<(NativeVideoSource, u32, u32)>) {
        *lock(&self.inner.screen) = source.map(|(source, max_width, max_height)| ScreenSink {
            source,
            max_width,
            max_height,
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
        let frame = {
            let guard = lock(&self.inner.screen);
            let Some(sink) = guard.as_ref() else {
                return false;
            };
            let mut i420 = bgra_to_i420(width, height, stride, data);
            let (w, h) = fit_within(width, height, sink.max_width, sink.max_height);
            if (w, h) != (i420.width(), i420.height()) {
                i420 = scale_i420(&mut i420, w, h);
            }
            let mut frame = VideoFrame::new(VideoRotation::VideoRotation0, i420);
            frame.timestamp_us = frame_timestamp(&self.inner.started);
            sink.source.capture_frame(&frame);
            frame
        };
        self.inner
            .counters
            .screen_frames_sent
            .fetch_add(1, Ordering::Relaxed);
        let local = lock(&self.inner.local_identity).clone();
        if let Some(id) = local {
            let mut buffer = frame.buffer;
            self.maybe_preview(&id, &mut buffer);
        }
        true
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

    pub fn remove_remote(&self, participant_id: &str) {
        lock(&self.inner.mixer).sources.remove(participant_id);
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
