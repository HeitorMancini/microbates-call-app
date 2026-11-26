//! Audio of the shared screen through WASAPI process loopback (Windows 10 2004+).
//!
//! A window is captured by including its process tree; a monitor is captured by excluding
//! this app's own process tree, so the voices played back by Sheeshcord never return to
//! the call.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use sheeshcord_core::BoxError;
use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

use crate::hub::{MediaHub, SAMPLES_PER_10MS, SAMPLE_RATE};

/// Which processes' audio is captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTarget {
    /// Only this process (and its children).
    IncludeProcess(u32),
    /// Everything except this process (and its children).
    ExcludeProcess(u32),
}

const CHANNELS: usize = 2;
const WAIT_MS: u32 = 100;

/// A running loopback capture; dropping it stops the capture thread.
pub struct ScreenAudioCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ScreenAudioCapture {
    /// Starts capturing `target` into [`MediaHub::push_screen_audio`]. Fails when process
    /// loopback is unavailable (older Windows) or the stream cannot be opened.
    pub fn start(target: AudioTarget, hub: MediaHub) -> Result<Self, BoxError> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("screen-audio".to_owned())
            .spawn({
                let stop = stop.clone();
                move || run(target, hub, stop, ready_tx)
            })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e.into())
            }
            Err(_) => Err("screen audio thread exited unexpectedly".into()),
        }
    }
}

impl Drop for ScreenAudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    target: AudioTarget,
    hub: MediaHub,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), String>>,
) {
    let opened = open(target);
    let (client, capture, event) = match opened {
        Ok(parts) => parts,
        Err(e) => {
            let _ = ready.send(Err(format!("falha ao capturar o áudio: {e}")));
            return;
        }
    };
    let _ = ready.send(Ok(()));

    let mut queue = VecDeque::<u8>::new();
    let mut samples = Vec::<i16>::new();
    let mut pending = Vec::<i16>::new();
    while !stop.load(Ordering::Relaxed) {
        // A timeout only means nothing is playing; it is also how `stop` is noticed.
        let _ = event.wait_for_event(WAIT_MS);
        queue.clear();
        match capture.read_from_device_to_deque(&mut queue) {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "screen audio capture stopped");
                break;
            }
        }
        if queue.is_empty() {
            continue;
        }
        let bytes = queue.make_contiguous();
        let floats: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        downmix_to_i16(&floats, CHANNELS, &mut samples);
        pending.extend_from_slice(&samples);
        let (chunks, rest) = pending.as_chunks::<SAMPLES_PER_10MS>();
        for chunk in chunks {
            hub.push_screen_audio(chunk);
        }
        pending = rest.to_vec();
    }
    let _ = client.stop_stream();
}

fn open(
    target: AudioTarget,
) -> Result<(AudioClient, wasapi::AudioCaptureClient, wasapi::Handle), BoxError> {
    wasapi::initialize_mta().ok()?;
    let (pid, include_tree) = match target {
        AudioTarget::IncludeProcess(pid) => (pid, true),
        AudioTarget::ExcludeProcess(pid) => (pid, false),
    };
    let format = WaveFormat::new(
        32,
        32,
        SampleType::Float,
        SAMPLE_RATE as usize,
        CHANNELS,
        None,
    );
    let mut client = AudioClient::new_application_loopback_client(pid, include_tree)?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
    };
    client.initialize_client(&format, Direction::Capture, mode, None)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    Ok((client, capture, event))
}

/// Averages interleaved `channels` into mono and converts to saturated 16-bit PCM.
pub(crate) fn downmix_to_i16(input: &[f32], channels: usize, out: &mut Vec<i16>) {
    out.clear();
    out.extend(input.chunks_exact(channels).map(|frame| {
        let mean = frame.iter().sum::<f32>() / channels as f32;
        (mean.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
    }));
}

/// Owner process of a window, for capturing only that app's audio.
#[cfg(windows)]
pub(crate) fn pid_for_hwnd(hwnd: isize) -> Option<u32> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    let mut pid = 0u32;
    // SAFETY: `pid` outlives the call; an invalid HWND just yields thread id 0.
    let thread = unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
    (thread != 0 && pid != 0).then_some(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_is_averaged_to_mono() {
        let mut out = Vec::new();
        downmix_to_i16(&[0.5, -0.5, 1.0, 0.0], 2, &mut out);
        assert_eq!(out, vec![0, i16::MAX / 2]);
    }

    #[test]
    fn out_of_range_samples_saturate() {
        let mut out = Vec::new();
        downmix_to_i16(&[1.5, 1.5, -1.5, -1.5], 2, &mut out);
        assert_eq!(out, vec![i16::MAX, -i16::MAX]);
    }

    #[test]
    fn a_trailing_partial_frame_is_ignored() {
        let mut out = vec![9];
        downmix_to_i16(&[0.0, 0.0, 0.3], 2, &mut out);
        assert_eq!(out, vec![0]);
    }
}
