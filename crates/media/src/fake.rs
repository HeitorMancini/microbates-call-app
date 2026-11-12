//! Synthetic media sources for tests/CI without audio or display hardware.
//!
//! * [`FakeAudio`]: "microphone" producing a 440 Hz sine and "speakers" that drain the
//!   playback mixer in real time.
//! * [`FakeScreen`]: moving color bars at the preset frame rate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sheeshcord_core::{
    AudioDevices, AudioSource, BoxError, Device, DeviceKind, ScreenPreset, ScreenSource,
    ScreenSourceInfo, ScreenSourceKind,
};

use crate::hub::{lock, MediaHub, SAMPLES_PER_10MS, SAMPLE_RATE};
use crate::video::fit_within;

pub const FAKE_INPUT_ID: &str = "fake-mic";
pub const FAKE_OUTPUT_ID: &str = "fake-speakers";
pub const FAKE_MONITOR_ID: &str = "fake:pattern";
pub const FAKE_WINDOW_ID: &str = "fake:window";

const TONE_HZ: f64 = 440.0;
const TONE_AMPLITUDE: f64 = 0.3 * i16::MAX as f64;

struct Worker {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Worker {
    fn spawn(name: &str, body: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || body(flag))
            .expect("failed to spawn fake media thread");
        Self { stop, thread }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.thread.join();
    }
}

/// Generates the `index`-th 10 ms chunk of the 440 Hz test tone (48 kHz mono).
pub fn tone_chunk(index: u64) -> Vec<i16> {
    let start = index * SAMPLES_PER_10MS as u64;
    (0..SAMPLES_PER_10MS as u64)
        .map(|i| {
            let t = (start + i) as f64 / f64::from(SAMPLE_RATE);
            (TONE_AMPLITUDE * (2.0 * std::f64::consts::PI * TONE_HZ * t).sin()) as i16
        })
        .collect()
}

#[derive(Default)]
struct AudioFlags {
    capture_muted: AtomicBool,
    playback_muted: AtomicBool,
}

pub struct FakeAudio {
    hub: MediaHub,
    flags: Arc<AudioFlags>,
    worker: Mutex<Option<Worker>>,
    selected: Mutex<(String, String)>,
}

impl FakeAudio {
    pub fn new(hub: MediaHub) -> Self {
        Self {
            hub,
            flags: Arc::default(),
            worker: Mutex::new(None),
            selected: Mutex::new((FAKE_INPUT_ID.to_owned(), FAKE_OUTPUT_ID.to_owned())),
        }
    }

    pub fn is_running(&self) -> bool {
        lock(&self.worker).is_some()
    }
}

impl Drop for FakeAudio {
    fn drop(&mut self) {
        if let Some(worker) = lock(&self.worker).take() {
            worker.stop();
        }
    }
}

#[async_trait]
impl AudioSource for FakeAudio {
    async fn list_devices(&self) -> Result<AudioDevices, BoxError> {
        Ok(AudioDevices {
            inputs: vec![Device {
                id: FAKE_INPUT_ID.to_owned(),
                name: "Tom de teste 440 Hz".to_owned(),
            }],
            outputs: vec![Device {
                id: FAKE_OUTPUT_ID.to_owned(),
                name: "Saída simulada".to_owned(),
            }],
        })
    }

    async fn select_device(&self, kind: DeviceKind, id: &str) -> Result<(), BoxError> {
        let mut selected = lock(&self.selected);
        match (kind, id) {
            (DeviceKind::Input, FAKE_INPUT_ID) => selected.0 = id.to_owned(),
            (DeviceKind::Output, FAKE_OUTPUT_ID) => selected.1 = id.to_owned(),
            _ => return Err(format!("unknown device {id}").into()),
        }
        Ok(())
    }

    async fn start(&self) -> Result<(), BoxError> {
        let mut worker = lock(&self.worker);
        if worker.is_some() {
            return Ok(());
        }
        let hub = self.hub.clone();
        let flags = Arc::clone(&self.flags);
        *worker = Some(Worker::spawn("fake-audio", move |stop| {
            let started = Instant::now();
            let mut chunk = 0u64;
            let mut playback = vec![0i16; SAMPLES_PER_10MS];
            while !stop.load(Ordering::Relaxed) {
                let due = (started.elapsed().as_millis() / 10) as u64;
                while chunk < due {
                    if !flags.capture_muted.load(Ordering::Relaxed) {
                        hub.push_mic(&tone_chunk(chunk));
                    }
                    hub.mix_playback(&mut playback);
                    if flags.playback_muted.load(Ordering::Relaxed) {
                        playback.fill(0);
                    }
                    chunk += 1;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }));
        Ok(())
    }

    async fn stop(&self) {
        let worker = lock(&self.worker).take();
        if let Some(worker) = worker {
            worker.stop();
        }
    }

    async fn set_capture_muted(&self, muted: bool) -> Result<(), BoxError> {
        self.flags.capture_muted.store(muted, Ordering::Relaxed);
        Ok(())
    }

    async fn set_playback_muted(&self, muted: bool) -> Result<(), BoxError> {
        self.flags.playback_muted.store(muted, Ordering::Relaxed);
        Ok(())
    }
}

/// BGRA test pattern: eight vertical color bars scrolling one bar width per second.
pub struct TestPattern {
    pub width: u32,
    pub height: u32,
    /// One row of the pattern repeated twice so a scrolled row is a single slice.
    row: Vec<u8>,
}

impl TestPattern {
    const BARS: [[u8; 4]; 8] = [
        [255, 255, 255, 255],
        [0, 255, 255, 255],
        [255, 255, 0, 255],
        [0, 255, 0, 255],
        [255, 0, 255, 255],
        [0, 0, 255, 255],
        [255, 0, 0, 255],
        [16, 16, 16, 255],
    ];

    pub fn new(width: u32, height: u32) -> Self {
        let bar = (width / 8).max(1);
        let row = (0..width * 2)
            .flat_map(|x| Self::BARS[((x / bar) % 8) as usize])
            .collect();
        Self { width, height, row }
    }

    /// Renders frame `index` at `fps` into `out` (tightly packed BGRA).
    pub fn render(&self, index: u64, fps: u32, out: &mut Vec<u8>) {
        let stride = (self.width * 4) as usize;
        out.resize(stride * self.height as usize, 0);
        let bar = u64::from((self.width / 8).max(1));
        let shift = ((index * bar / u64::from(fps.max(1))) % u64::from(self.width)) as usize * 4;
        for row in out.chunks_exact_mut(stride) {
            row.copy_from_slice(&self.row[shift..shift + stride]);
        }
    }
}

pub struct FakeScreen {
    hub: MediaHub,
    max_size: (u32, u32),
    worker: Mutex<Option<Worker>>,
}

impl FakeScreen {
    /// Frames are rendered at 640x360 (scaled down further for smaller presets) to keep
    /// CPU usage low in debug builds.
    pub fn new(hub: MediaHub) -> Self {
        Self::with_size(hub, 640, 360)
    }

    pub fn with_size(hub: MediaHub, width: u32, height: u32) -> Self {
        Self {
            hub,
            max_size: (width, height),
            worker: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        lock(&self.worker).is_some()
    }
}

impl Drop for FakeScreen {
    fn drop(&mut self) {
        if let Some(worker) = lock(&self.worker).take() {
            worker.stop();
        }
    }
}

#[async_trait]
impl ScreenSource for FakeScreen {
    async fn list_sources(&self) -> Result<Vec<ScreenSourceInfo>, BoxError> {
        Ok(vec![
            ScreenSourceInfo {
                id: FAKE_MONITOR_ID.to_owned(),
                name: "Padrão de teste".to_owned(),
                kind: ScreenSourceKind::Monitor,
            },
            ScreenSourceInfo {
                id: FAKE_WINDOW_ID.to_owned(),
                name: "Janela de teste".to_owned(),
                kind: ScreenSourceKind::Window,
            },
        ])
    }

    async fn start(&self, source_id: &str, preset: ScreenPreset) -> Result<(), BoxError> {
        if source_id != FAKE_MONITOR_ID && source_id != FAKE_WINDOW_ID {
            return Err(format!("unknown source {source_id}").into());
        }
        self.stop().await;
        let (width, height) = fit_within(
            self.max_size.0,
            self.max_size.1,
            preset.width(),
            preset.height(),
        );
        let fps = preset.fps();
        let hub = self.hub.clone();
        *lock(&self.worker) = Some(Worker::spawn("fake-screen", move |stop| {
            let pattern = TestPattern::new(width, height);
            let interval = Duration::from_secs(1) / fps;
            let started = Instant::now();
            let mut frame = Vec::new();
            let mut index = 0u64;
            while !stop.load(Ordering::Relaxed) {
                pattern.render(index, fps, &mut frame);
                hub.push_screen_bgra(width, height, width * 4, &frame);
                index += 1;
                let next = started + interval * u32::try_from(index).unwrap_or(u32::MAX);
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
        }));
        Ok(())
    }

    async fn stop(&self) {
        let worker = lock(&self.worker).take();
        if let Some(worker) = worker {
            worker.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_is_continuous_440hz() {
        let a = tone_chunk(0);
        let b = tone_chunk(1);
        assert_eq!(a.len(), SAMPLES_PER_10MS);
        assert_eq!(a[0], 0);
        // 440 Hz over 10 ms = 4.4 periods -> ~8-9 zero crossings per chunk.
        let crossings = a.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
        assert!((8..=9).contains(&crossings), "{crossings}");
        let peak = a.iter().chain(&b).map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 9000);
        assert!(
            (i32::from(a[479]) - i32::from(b[0])).abs() < 2000,
            "no discontinuity"
        );
    }

    #[test]
    fn pattern_scrolls() {
        let pattern = TestPattern::new(64, 4);
        let (mut f0, mut f1) = (Vec::new(), Vec::new());
        pattern.render(0, 30, &mut f0);
        pattern.render(15, 30, &mut f1);
        assert_eq!(f0.len(), 64 * 4 * 4);
        assert_eq!(&f0[..4], &TestPattern::BARS[0]);
        assert_ne!(f0, f1);
    }

    #[tokio::test]
    async fn fake_audio_feeds_the_hub_and_respects_mute() {
        let hub = MediaHub::default();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1000);
        hub.set_mic_sink(Some(tx));
        let audio = FakeAudio::new(hub.clone());
        audio.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        audio.set_capture_muted(true).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let sent = hub.stats().mic_chunks_sent;
        tokio::time::sleep(Duration::from_millis(60)).await;
        audio.stop().await;
        assert!(sent >= 3, "sent {sent}");
        assert_eq!(hub.stats().mic_chunks_sent, sent, "muted mic sends nothing");
        assert!(hub.stats().played_samples > 0);
        assert!(rx.try_recv().is_ok());
        assert!(audio
            .select_device(DeviceKind::Input, "nope")
            .await
            .is_err());
    }
}
