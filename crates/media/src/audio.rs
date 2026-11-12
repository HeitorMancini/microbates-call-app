//! Microphone capture and speaker playback with `cpal`, processed by WebRTC's audio
//! processing module (echo cancellation, noise suppression, AGC, high-pass) that ships
//! inside the LiveKit SDK's libwebrtc. Opus encoding/decoding happens in libwebrtc too.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use livekit::webrtc::native::apm::AudioProcessingModule;
use sheeshcord_core::{AudioDevices, AudioSource, BoxError, Device, DeviceKind};
use tokio::sync::oneshot;

use crate::hub::{lock, MediaHub, SAMPLES_PER_10MS, SAMPLE_RATE};

/// Which WebRTC audio processing stages run on the microphone signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioProcessing {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
    pub high_pass_filter: bool,
    /// Estimated delay between playing a sample and capturing its echo.
    pub stream_delay_ms: i32,
}

impl Default for AudioProcessing {
    fn default() -> Self {
        Self {
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
            high_pass_filter: true,
            stream_delay_ms: 60,
        }
    }
}

impl AudioProcessing {
    fn enabled(&self) -> bool {
        self.echo_cancellation
            || self.noise_suppression
            || self.auto_gain_control
            || self.high_pass_filter
    }
}

/// Streaming linear resampler for mono `f32`.
#[derive(Debug, Clone)]
pub struct Resampler {
    step: f64,
    t: f64,
    prev: f32,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            step: f64::from(from_rate) / f64::from(to_rate),
            t: 0.0,
            prev: 0.0,
        }
    }

    pub fn push(&mut self, sample: f32, mut emit: impl FnMut(f32)) {
        while self.t < 1.0 {
            emit(self.prev + (sample - self.prev) * self.t as f32);
            self.t += self.step;
        }
        self.t -= 1.0;
        self.prev = sample;
    }
}

fn to_i16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

#[derive(Default)]
struct Flags {
    capture_muted: AtomicBool,
    playback_muted: AtomicBool,
}

type SharedApm = Arc<Mutex<AudioProcessingModule>>;

struct CapturePipeline {
    channels: usize,
    resampler: Resampler,
    pending: Vec<i16>,
    apm: Option<SharedApm>,
    hub: MediaHub,
    flags: Arc<Flags>,
}

impl CapturePipeline {
    fn process<T>(&mut self, data: &[T])
    where
        T: Sample,
        f32: FromSample<T>,
    {
        let pending = &mut self.pending;
        for frame in data.chunks(self.channels) {
            let mono = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / frame.len() as f32;
            self.resampler.push(mono, |v| pending.push(to_i16(v)));
        }
        while self.pending.len() >= SAMPLES_PER_10MS {
            let mut chunk: Vec<i16> = self.pending.drain(..SAMPLES_PER_10MS).collect();
            if let Some(apm) = &self.apm {
                if let Err(e) = lock(apm).process_stream(&mut chunk, SAMPLE_RATE as i32, 1) {
                    tracing::trace!(error = %e.message, "apm capture");
                }
            }
            if !self.flags.capture_muted.load(Ordering::Relaxed) {
                self.hub.push_mic(&chunk);
            }
        }
    }
}

struct PlaybackPipeline {
    channels: usize,
    resampler: Resampler,
    ready: VecDeque<f32>,
    mix: Vec<i16>,
    apm: Option<SharedApm>,
    hub: MediaHub,
    flags: Arc<Flags>,
}

impl PlaybackPipeline {
    fn fill<T>(&mut self, out: &mut [T])
    where
        T: Sample + FromSample<f32>,
    {
        let frames = out.len() / self.channels;
        while self.ready.len() < frames {
            self.hub.mix_playback(&mut self.mix);
            if self.flags.playback_muted.load(Ordering::Relaxed) {
                self.mix.fill(0);
            }
            if let Some(apm) = &self.apm {
                let mut reverse = self.mix.clone();
                if let Err(e) =
                    lock(apm).process_reverse_stream(&mut reverse, SAMPLE_RATE as i32, 1)
                {
                    tracing::trace!(error = %e.message, "apm reverse");
                }
            }
            let ready = &mut self.ready;
            for &s in &self.mix {
                self.resampler
                    .push(f32::from(s) / f32::from(i16::MAX), |v| ready.push_back(v));
            }
        }
        for frame in out.chunks_mut(self.channels) {
            let v = T::from_sample(self.ready.pop_front().unwrap_or(0.0));
            frame.fill(v);
        }
    }
}

#[derive(Default, Clone)]
struct Selection {
    input: Option<String>,
    output: Option<String>,
}

struct Engine {
    stop: std_mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

/// [`AudioSource`] using the system's default host (WASAPI on Windows).
pub struct CpalAudio {
    hub: MediaHub,
    processing: AudioProcessing,
    flags: Arc<Flags>,
    selection: Mutex<Selection>,
    engine: Mutex<Option<Engine>>,
}

impl CpalAudio {
    pub fn new(hub: MediaHub) -> Self {
        Self::with_processing(hub, AudioProcessing::default())
    }

    pub fn with_processing(hub: MediaHub, processing: AudioProcessing) -> Self {
        Self {
            hub,
            processing,
            flags: Arc::default(),
            selection: Mutex::default(),
            engine: Mutex::new(None),
        }
    }

    fn stop_engine(&self) {
        let engine = lock(&self.engine).take();
        if let Some(engine) = engine {
            let _ = engine.stop.send(());
            let _ = engine.thread.join();
        }
    }
}

impl Drop for CpalAudio {
    fn drop(&mut self) {
        self.stop_engine();
    }
}

fn device_id(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.to_string())
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| "Dispositivo sem nome".to_owned())
}

fn list(kind: DeviceKind) -> Result<Vec<Device>, BoxError> {
    let host = cpal::default_host();
    let devices: Vec<cpal::Device> = match kind {
        DeviceKind::Input => host.input_devices()?.collect(),
        DeviceKind::Output => host.output_devices()?.collect(),
    };
    Ok(devices
        .iter()
        .filter_map(|d| {
            Some(Device {
                id: device_id(d)?,
                name: device_name(d),
            })
        })
        .collect())
}

fn find_device(kind: DeviceKind, id: Option<&str>) -> Result<cpal::Device, BoxError> {
    let host = cpal::default_host();
    let found = match id {
        Some(id) => {
            let devices: Vec<cpal::Device> = match kind {
                DeviceKind::Input => host.input_devices()?.collect(),
                DeviceKind::Output => host.output_devices()?.collect(),
            };
            devices
                .into_iter()
                .find(|d| device_id(d).as_deref() == Some(id))
        }
        None => match kind {
            DeviceKind::Input => host.default_input_device(),
            DeviceKind::Output => host.default_output_device(),
        },
    };
    found.ok_or_else(|| format!("dispositivo de áudio não encontrado: {id:?}").into())
}

fn on_stream_error(err: cpal::Error) {
    tracing::warn!(error = %err, "audio stream error");
}

fn build_input<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut pipeline: CapturePipeline,
) -> Result<cpal::Stream, BoxError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _| pipeline.process(data),
        on_stream_error,
        None,
    )?)
}

fn build_output<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut pipeline: PlaybackPipeline,
) -> Result<cpal::Stream, BoxError>
where
    T: SizedSample + FromSample<f32>,
{
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| pipeline.fill(data),
        on_stream_error,
        None,
    )?)
}

struct Shared {
    hub: MediaHub,
    flags: Arc<Flags>,
    apm: Option<SharedApm>,
}

fn open_input(id: Option<&str>, shared: &Shared) -> Result<cpal::Stream, BoxError> {
    let device = find_device(DeviceKind::Input, id)?;
    let supported = device.default_input_config()?;
    let config = supported.config();
    let pipeline = CapturePipeline {
        channels: usize::from(config.channels.max(1)),
        resampler: Resampler::new(config.sample_rate, SAMPLE_RATE),
        pending: Vec::with_capacity(SAMPLES_PER_10MS * 2),
        apm: shared.apm.clone(),
        hub: shared.hub.clone(),
        flags: Arc::clone(&shared.flags),
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_input::<f32>(&device, config, pipeline)?,
        SampleFormat::I16 => build_input::<i16>(&device, config, pipeline)?,
        SampleFormat::I32 => build_input::<i32>(&device, config, pipeline)?,
        SampleFormat::U16 => build_input::<u16>(&device, config, pipeline)?,
        other => return Err(format!("formato de captura não suportado: {other:?}").into()),
    };
    stream.play()?;
    Ok(stream)
}

fn open_output(id: Option<&str>, shared: &Shared) -> Result<cpal::Stream, BoxError> {
    let device = find_device(DeviceKind::Output, id)?;
    let supported = device.default_output_config()?;
    let config = supported.config();
    let pipeline = PlaybackPipeline {
        channels: usize::from(config.channels.max(1)),
        resampler: Resampler::new(SAMPLE_RATE, config.sample_rate),
        ready: VecDeque::with_capacity(SAMPLES_PER_10MS * 8),
        mix: vec![0; SAMPLES_PER_10MS],
        apm: shared.apm.clone(),
        hub: shared.hub.clone(),
        flags: Arc::clone(&shared.flags),
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_output::<f32>(&device, config, pipeline)?,
        SampleFormat::I16 => build_output::<i16>(&device, config, pipeline)?,
        SampleFormat::I32 => build_output::<i32>(&device, config, pipeline)?,
        SampleFormat::U16 => build_output::<u16>(&device, config, pipeline)?,
        other => return Err(format!("formato de saída não suportado: {other:?}").into()),
    };
    stream.play()?;
    Ok(stream)
}

#[async_trait]
impl AudioSource for CpalAudio {
    async fn list_devices(&self) -> Result<AudioDevices, BoxError> {
        tokio::task::spawn_blocking(|| {
            Ok(AudioDevices {
                inputs: list(DeviceKind::Input)?,
                outputs: list(DeviceKind::Output)?,
            })
        })
        .await?
    }

    async fn select_device(&self, kind: DeviceKind, id: &str) -> Result<(), BoxError> {
        let owned = id.to_owned();
        let known = tokio::task::spawn_blocking(move || {
            list(kind).map(|devices| devices.iter().any(|d| d.id == owned))
        })
        .await??;
        if !known {
            return Err(format!("dispositivo de áudio não encontrado: {id}").into());
        }
        {
            let mut selection = lock(&self.selection);
            match kind {
                DeviceKind::Input => selection.input = Some(id.to_owned()),
                DeviceKind::Output => selection.output = Some(id.to_owned()),
            }
        }
        if lock(&self.engine).is_some() {
            self.stop_engine();
            self.start().await?;
        }
        Ok(())
    }

    async fn start(&self) -> Result<(), BoxError> {
        if lock(&self.engine).is_some() {
            return Ok(());
        }
        let selection = lock(&self.selection).clone();
        let processing = self.processing;
        let shared = Shared {
            hub: self.hub.clone(),
            flags: Arc::clone(&self.flags),
            apm: processing.enabled().then(|| {
                let mut apm = AudioProcessingModule::new(
                    processing.echo_cancellation,
                    processing.auto_gain_control,
                    processing.high_pass_filter,
                    processing.noise_suppression,
                );
                let _ = apm.set_stream_delay_ms(processing.stream_delay_ms);
                Arc::new(Mutex::new(apm))
            }),
        };
        let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();
        let (stop_tx, stop_rx) = std_mpsc::channel::<()>();
        // cpal streams are not `Send` on every platform, so they live on their own thread.
        let thread = std::thread::Builder::new()
            .name("cpal-audio".to_owned())
            .spawn(move || {
                let input = open_input(selection.input.as_deref(), &shared);
                let output = open_output(selection.output.as_deref(), &shared);
                let result = match (&input, &output) {
                    (Err(i), Err(o)) => Err(format!("microfone: {i}; saída: {o}")),
                    _ => {
                        if let Err(e) = &input {
                            tracing::warn!(error = %e, "microphone unavailable; listening only");
                        }
                        if let Err(e) = &output {
                            tracing::warn!(error = %e, "speakers unavailable; talking only");
                        }
                        Ok(())
                    }
                };
                let failed = result.is_err();
                let _ = ready_tx.send(result);
                if !failed {
                    let _ = stop_rx.recv();
                }
                drop((input, output));
            })?;
        match ready_rx.await {
            Ok(Ok(())) => {
                *lock(&self.engine) = Some(Engine {
                    stop: stop_tx,
                    thread,
                });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e.into())
            }
            Err(_) => Err("audio thread exited unexpectedly".into()),
        }
    }

    async fn stop(&self) {
        self.stop_engine();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_converts_rates() {
        for (from, to) in [
            (44_100, 48_000),
            (48_000, 48_000),
            (48_000, 16_000),
            (96_000, 48_000),
        ] {
            let mut r = Resampler::new(from, to);
            let mut out = 0usize;
            for i in 0..from {
                r.push((i as f32 * 0.01).sin(), |_| out += 1);
            }
            let diff = out.abs_diff(to as usize);
            assert!(diff <= 2, "{from}->{to}: {out}");
        }
    }

    #[test]
    fn capture_pipeline_downmixes_into_10ms_chunks() {
        let hub = MediaHub::default();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        hub.set_mic_sink(Some(tx));
        let mut pipeline = CapturePipeline {
            channels: 2,
            resampler: Resampler::new(48_000, 48_000),
            pending: Vec::new(),
            apm: None,
            hub,
            flags: Arc::default(),
        };
        pipeline.process(&vec![0.5f32; SAMPLES_PER_10MS * 2 * 2 + 2]);
        let first = rx.try_recv().unwrap();
        assert_eq!(first.len(), SAMPLES_PER_10MS);
        assert!((i32::from(first[100]) - i32::from(i16::MAX / 2)).abs() < 2);
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn playback_pipeline_upmixes_and_silences_when_deafened() {
        let hub = MediaHub::default();
        hub.push_remote_audio("a", &vec![i16::MAX / 2; SAMPLES_PER_10MS * 8]);
        let flags = Arc::new(Flags::default());
        let mut pipeline = PlaybackPipeline {
            channels: 2,
            resampler: Resampler::new(48_000, 48_000),
            ready: VecDeque::new(),
            mix: vec![0; SAMPLES_PER_10MS],
            apm: None,
            hub,
            flags: Arc::clone(&flags),
        };
        let mut out = vec![0f32; 960];
        pipeline.fill(&mut out);
        assert!((out[200] - 0.5).abs() < 0.01 && out[200] == out[201]);
        flags.playback_muted.store(true, Ordering::Relaxed);
        pipeline.ready.clear();
        pipeline.fill(&mut out);
        // The resampler lags one sample behind, so skip the first (interpolated) frame.
        assert!(out[2..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn apm_accepts_10ms_chunks() {
        let mut apm = AudioProcessingModule::new(true, true, true, true);
        let mut chunk: Vec<i16> = (0..SAMPLES_PER_10MS)
            .map(|i| ((i as f32 * 0.0576).sin() * 8000.0) as i16)
            .collect();
        apm.process_reverse_stream(&mut chunk.clone(), SAMPLE_RATE as i32, 1)
            .unwrap();
        apm.process_stream(&mut chunk, SAMPLE_RATE as i32, 1)
            .unwrap();
    }
}
