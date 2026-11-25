//! [`Transport`] backed by the LiveKit Rust SDK (libwebrtc).

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::StreamExt;
use livekit::options::{
    DegradationPreference, TrackPublishOptions, VideoCodec, VideoEncoderBackend, VideoEncoding,
};
use livekit::prelude::*;
use livekit::rtc_engine::lk_runtime::LkRuntime;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::AudioFrame;
use livekit::webrtc::stats::{QualityLimitationReason, RtcStats};
use livekit::webrtc::video_codec::VideoCodec as EncoderCodec;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use sheeshcord_core::{
    BoxError, RemoteParticipant as CoreParticipant, ScreenPreset, Session, Transport,
    TransportEvent,
};
use tokio::sync::{mpsc, Mutex};
use tokio::task::{AbortHandle, JoinHandle, JoinSet};

use crate::hub::{screen_audio_key, MediaHub, ScreenOutput, SAMPLE_RATE};
use crate::hwenc::{self, HwConfig, HwProbe, HwScreenEncoder};

/// Identity prefix of the WebView's subscribe-only video client (must match the
/// token-server's `VIEWER_IDENTITY_PREFIX`); never shown in the roster.
pub const VIEWER_IDENTITY_PREFIX: &str = "viewer~";

/// Encoder settings for the screen-share track.
///
/// AV1 is the default (best quality per bit for screen content), H264 the fallback. With
/// `hardware_encoder` the screen is encoded on the GPU (Media Foundation: NVIDIA NVENC,
/// AMD AMF or Intel QSV) in the first of those codecs the GPU can encode. Otherwise, or
/// when no GPU encoder works, libwebrtc encodes in software (webrtc-sys 0.3 has no Windows
/// hardware encoder: libaom / OpenH264), with VP8 as the last resort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportOptions {
    pub screen_codec: VideoCodec,
    pub fallback_codec: VideoCodec,
    /// Encode the screen on the GPU when this machine has a working hardware encoder.
    pub hardware_encoder: bool,
    /// Decode remote video natively into the hub's RGBA previews. The app leaves this off
    /// because the WebView renders video itself (hardware decode, no JPEG/IPC round trip).
    pub subscribe_video: bool,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            screen_codec: VideoCodec::AV1,
            fallback_codec: VideoCodec::H264,
            hardware_encoder: true,
            subscribe_video: false,
        }
    }
}

/// Which encoder produces the published screen track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenEncoderInfo {
    pub codec: VideoCodec,
    /// Hardware MFT name (e.g. "AMDav1Encoder") or "libwebrtc" for software.
    pub encoder: String,
    pub hardware: bool,
}

impl ScreenEncoderInfo {
    fn software(codec: VideoCodec) -> Self {
        Self {
            codec,
            encoder: "libwebrtc".to_owned(),
            hardware: false,
        }
    }
}

/// Codecs to probe on the GPU, in preference order (empty: hardware encoding disabled).
fn hardware_candidates(options: &TransportOptions) -> Vec<VideoCodec> {
    if !options.hardware_encoder {
        return Vec::new();
    }
    hwenc::hardware_codecs(&[options.screen_codec, options.fallback_codec])
}

/// Idle capture time after which the last screen frame is re-sent.
const SCREEN_REPEAT_IDLE: Duration = Duration::from_millis(250);
/// How often the screen encoder stats are checked.
const SCREEN_STATS_EVERY: Duration = Duration::from_secs(5);
/// Consecutive CPU-limited checks before switching to the fallback codec.
const CPU_LIMITED_CHECKS: u32 = 3;
/// The Windows libwebrtc build has no hardware encoders, so a CPU-limited AV1 encoder
/// would only get slower on OpenH264 (load-test 1080p30: AV1 ~23 fps, VP8 ~11, H.264 ~6).
const SWITCH_ON_CPU_LIMIT: bool = !cfg!(windows);

/// Codecs to try, in order, for the screen track: `preferred`, then `fallback`, then VP8,
/// skipping those the host cannot encode (LiveKit would silently swap them for VP8).
fn screen_codec_order(
    preferred: VideoCodec,
    fallback: VideoCodec,
    producible: &[EncoderCodec],
) -> Vec<VideoCodec> {
    let mut order = Vec::new();
    for codec in [preferred, fallback, VideoCodec::VP8] {
        let encodable = codec == VideoCodec::VP8 || producible.contains(&codec.into());
        if encodable && !order.contains(&codec) {
            order.push(codec);
        }
    }
    order
}

fn screen_publish_options(
    preset: ScreenPreset,
    codec: VideoCodec,
    backend: VideoEncoderBackend,
) -> TrackPublishOptions {
    TrackPublishOptions {
        source: TrackSource::Screenshare,
        video_codec: codec,
        simulcast: false,
        video_encoding: Some(VideoEncoding {
            max_bitrate: preset_max_bitrate(preset),
            max_framerate: f64::from(preset.fps()),
        }),
        video_encoder: backend,
        // Games: keep the frame rate smooth and let the resolution give way first.
        degradation_preference: Some(DegradationPreference::MaintainFramerate),
        ..Default::default()
    }
}

/// Publishes `source` with the first codec in `codecs` that LiveKit accepts.
async fn publish_screen_track(
    local: &LocalParticipant,
    source: &NativeVideoSource,
    preset: ScreenPreset,
    codecs: &[VideoCodec],
    backend: VideoEncoderBackend,
) -> Result<(LocalTrackPublication, LocalVideoTrack, VideoCodec), BoxError> {
    let mut last_error: Option<BoxError> = None;
    for &codec in codecs {
        let track =
            LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(source.clone()));
        match local
            .publish_track(
                LocalTrack::Video(track.clone()),
                screen_publish_options(preset, codec, backend),
            )
            .await
        {
            Ok(publication) => {
                tracing::info!(codec = codec.as_str(), "screen published");
                return Ok((publication, track, codec));
            }
            Err(e) => {
                tracing::warn!(error = %e, codec = codec.as_str(), "screen publish failed; trying the next codec");
                last_error = Some(e.into());
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "no screen codec available".into()))
}

/// Upper bound for the screen-share bitrate of each preset (bits per second).
pub fn preset_max_bitrate(preset: ScreenPreset) -> u64 {
    match preset {
        ScreenPreset::P1080Fps60 => 9_000_000,
        ScreenPreset::P1080Fps30 => 6_000_000,
        ScreenPreset::P720Fps30 => 3_000_000,
    }
}

struct ScreenTrack {
    publication: LocalTrackPublication,
    /// Frame repeats, encoder stats and the codec / software fallback.
    maintenance: JoinHandle<()>,
    info: ScreenEncoderInfo,
    /// The GPU encoder thread, while the screen is hardware encoded.
    hw: Option<Arc<HwScreenEncoder>>,
    audio: Option<ScreenAudioTrack>,
}

/// The audio track published next to the screen video.
struct ScreenAudioTrack {
    publication: LocalTrackPublication,
    pump: JoinHandle<()>,
}

/// Publishes the screen-share audio track and starts feeding it from the hub. `None`
/// (after a warning) when it cannot be published: the video is unaffected.
async fn publish_screen_audio(
    local: &LocalParticipant,
    hub: &MediaHub,
) -> Option<ScreenAudioTrack> {
    let source = NativeAudioSource::new(
        AudioSourceOptions {
            echo_cancellation: false,
            noise_suppression: false,
            auto_gain_control: false,
        },
        SAMPLE_RATE,
        1,
        100,
    );
    let track =
        LocalAudioTrack::create_audio_track("screen-audio", RtcAudioSource::Native(source.clone()));
    let publication = local
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::ScreenshareAudio,
                ..Default::default()
            },
        )
        .await
        .map_err(
            |e| tracing::warn!(error = %e, "screen audio publish failed; sharing without audio"),
        )
        .ok()?;
    let (tx, mut rx) = mpsc::channel::<Vec<i16>>(50);
    let pump = tokio::spawn(async move {
        while let Some(samples) = rx.recv().await {
            let samples_per_channel = samples.len() as u32;
            let frame = AudioFrame {
                data: samples.into(),
                sample_rate: SAMPLE_RATE,
                num_channels: 1,
                samples_per_channel,
            };
            if let Err(e) = source.capture_frame(&frame).await {
                tracing::warn!(error = %e, "failed to push screen audio");
            }
        }
    });
    hub.set_screen_audio_sink(Some(tx));
    Some(ScreenAudioTrack { publication, pump })
}

impl ScreenTrack {
    /// Stops the audio pump and removes the audio track; the video track is left alone.
    async fn stop_audio(&mut self, hub: &MediaHub, local: &LocalParticipant) {
        hub.set_screen_audio_sink(None);
        if let Some(audio) = self.audio.take() {
            audio.pump.abort();
            let _ = local.unpublish_track(&audio.publication.sid()).await;
        }
    }
}

struct Active {
    room: Room,
    mic: LocalTrackPublication,
    screen: Option<ScreenTrack>,
    events: JoinHandle<()>,
    mic_pump: JoinHandle<()>,
}

impl Active {
    fn abort_tasks(&self) {
        self.events.abort();
        self.mic_pump.abort();
        if let Some(screen) = &self.screen {
            screen.maintenance.abort();
            if let Some(audio) = &screen.audio {
                audio.pump.abort();
            }
        }
    }
}

type SharedActive = Arc<Mutex<Option<Active>>>;

/// LiveKit session: publishes the microphone (fed by [`MediaHub::push_mic`]) and, on
/// demand, the screen (fed by [`MediaHub::push_screen_bgra`]); remote audio/video is
/// delivered back into the hub.
pub struct LiveKitTransport {
    hub: MediaHub,
    options: TransportOptions,
    active: SharedActive,
}

impl LiveKitTransport {
    pub fn new(hub: MediaHub) -> Self {
        Self::with_options(hub, TransportOptions::default())
    }

    pub fn with_options(hub: MediaHub, options: TransportOptions) -> Self {
        Self {
            hub,
            options,
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub fn hub(&self) -> &MediaHub {
        &self.hub
    }

    /// The encoder of the currently published screen track, if any.
    pub async fn screen_encoder(&self) -> Option<ScreenEncoderInfo> {
        let guard = self.active.lock().await;
        Some(guard.as_ref()?.screen.as_ref()?.info.clone())
    }

    /// Test hook: makes the active GPU screen encoder report a failure.
    #[doc(hidden)]
    pub async fn fail_hardware_encoder_for_test(&self) {
        let guard = self.active.lock().await;
        if let Some(hw) = guard
            .as_ref()
            .and_then(|a| a.screen.as_ref())
            .and_then(|s| s.hw.as_ref())
        {
            hw.mark_failed();
        }
    }

    /// Probes the GPU and publishes a pre-encoded screen track fed by it. `None` when no
    /// hardware encoder works or LiveKit rejects the track (the caller uses software).
    async fn publish_screen_hardware(
        &self,
        local: &LocalParticipant,
        preset: ScreenPreset,
    ) -> Option<HwPublish> {
        let candidates = hardware_candidates(&self.options);
        if candidates.is_empty() {
            return None;
        }
        let (width, height) = (preset.width(), preset.height());
        let cfg = HwConfig {
            fps: preset.fps(),
            max_bitrate_bps: preset_max_bitrate(preset) as u32,
        };
        let HwProbe {
            codec,
            encoder,
            vendor,
        } = tokio::task::spawn_blocking(move || hwenc::probe(&candidates, width, height, cfg))
            .await
            .ok()??;
        let source = NativeVideoSource::new_encoded(VideoResolution { width, height });
        let (publication, track, codec) = publish_screen_track(
            local,
            &source,
            preset,
            &[codec],
            VideoEncoderBackend::PreEncoded,
        )
        .await
        .map_err(|e| tracing::warn!(error = %e, "pre-encoded screen publish failed"))
        .ok()?;
        tracing::info!(codec = codec.as_str(), encoder = %encoder, ?vendor, "screen encoding on the GPU");
        Some(HwPublish {
            publication,
            track,
            encoder: Arc::new(HwScreenEncoder::start(codec, encoder.clone(), cfg, source)),
            info: ScreenEncoderInfo {
                codec,
                encoder,
                hardware: true,
            },
        })
    }

    async fn close(&self, active: Active) {
        active.abort_tasks();
        self.hub.set_mic_sink(None);
        self.hub.set_screen_sink(None);
        self.hub.set_local_identity(None);
        self.hub.clear_remotes();
        if let Err(e) = active.room.close().await {
            tracing::debug!(error = %e, "room close");
        }
    }
}

impl Drop for LiveKitTransport {
    fn drop(&mut self) {
        // Best effort: the screen task only holds a `Weak`, so it also stops on its own.
        if let Ok(mut guard) = self.active.try_lock() {
            if let Some(active) = guard.take() {
                active.abort_tasks();
            }
        }
    }
}

fn identity(p: &RemoteParticipant) -> String {
    p.identity().to_string()
}

/// The WebView's subscribe-only video client; never shown in the roster.
fn is_viewer(p: &RemoteParticipant) -> bool {
    p.identity().as_str().starts_with(VIEWER_IDENTITY_PREFIX)
}

fn subscribe_wanted(publication: &RemoteTrackPublication, subscribe_video: bool) {
    let wanted = subscribe_video || publication.kind() == TrackKind::Audio;
    publication.set_subscribed(wanted);
}

fn has_source(p: &RemoteParticipant, source: TrackSource, unmuted_only: bool) -> bool {
    p.track_publications().values().any(|publication| {
        publication.source() == source && !(unmuted_only && publication.is_muted())
    })
}

fn describe(p: &RemoteParticipant) -> CoreParticipant {
    let name = p.name();
    CoreParticipant {
        id: identity(p),
        name: if name.is_empty() { identity(p) } else { name },
        muted: !has_source(p, TrackSource::Microphone, true),
        sharing_screen: has_source(p, TrackSource::Screenshare, false),
    }
}

#[async_trait]
impl Transport for LiveKitTransport {
    async fn connect(&self, url: &str, token: &str) -> Result<Session, BoxError> {
        let mut guard = self.active.lock().await;
        if let Some(previous) = guard.take() {
            self.close(previous).await;
        }

        let mut options = RoomOptions::default();
        options.auto_subscribe = self.options.subscribe_video;
        options.dynacast = true;
        let (room, room_events) = Room::connect(url, token, options).await?;
        let local = room.local_participant();
        let local_identity = local.identity().to_string();

        let source = NativeAudioSource::new(
            AudioSourceOptions {
                echo_cancellation: false,
                noise_suppression: false,
                auto_gain_control: false,
            },
            SAMPLE_RATE,
            1,
            100,
        );
        let track = LocalAudioTrack::create_audio_track(
            "microphone",
            RtcAudioSource::Native(source.clone()),
        );
        let mic = match local
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
        {
            Ok(publication) => publication,
            Err(e) => {
                let _ = room.close().await;
                return Err(e.into());
            }
        };

        let (mic_tx, mut mic_rx) = mpsc::channel::<Vec<i16>>(50);
        let mic_pump = tokio::spawn(async move {
            while let Some(samples) = mic_rx.recv().await {
                let samples_per_channel = samples.len() as u32;
                let frame = AudioFrame {
                    data: samples.into(),
                    sample_rate: SAMPLE_RATE,
                    num_channels: 1,
                    samples_per_channel,
                };
                if let Err(e) = source.capture_frame(&frame).await {
                    tracing::warn!(error = %e, "failed to push microphone audio");
                }
            }
        });
        self.hub.set_mic_sink(Some(mic_tx));
        self.hub.set_local_identity(Some(local_identity.clone()));

        let remotes = room.remote_participants();
        let participants = remotes
            .values()
            .filter(|p| !is_viewer(p))
            .map(describe)
            .collect();
        if !self.options.subscribe_video {
            for p in remotes.values().filter(|p| !is_viewer(p)) {
                for publication in p.track_publications().values() {
                    subscribe_wanted(publication, false);
                }
            }
        }
        let (tx, rx) = mpsc::channel(256);
        let events = tokio::spawn(pump_events(
            room_events,
            tx,
            self.hub.clone(),
            self.options.subscribe_video,
        ));

        *guard = Some(Active {
            room,
            mic,
            screen: None,
            events,
            mic_pump,
        });
        Ok(Session {
            local_identity,
            participants,
            events: rx,
        })
    }

    async fn disconnect(&self) {
        let active = self.active.lock().await.take();
        if let Some(active) = active {
            self.close(active).await;
        }
    }

    async fn set_microphone_muted(&self, muted: bool) -> Result<(), BoxError> {
        let guard = self.active.lock().await;
        let active = guard.as_ref().ok_or("not connected")?;
        if muted {
            active.mic.mute();
        } else {
            active.mic.unmute();
        }
        Ok(())
    }

    async fn publish_screen(&self, preset: ScreenPreset, with_audio: bool) -> Result<(), BoxError> {
        let mut guard = self.active.lock().await;
        let active = guard.as_mut().ok_or("not connected")?;
        if let Some(mut screen) = active.screen.take() {
            screen.maintenance.abort();
            self.hub.set_screen_sink(None);
            let local = active.room.local_participant();
            screen.stop_audio(&self.hub, &local).await;
            let _ = local.unpublish_track(&screen.publication.sid()).await;
        }

        let (width, height) = (preset.width(), preset.height());
        let local = active.room.local_participant();
        if let Some(HwPublish {
            publication,
            track,
            encoder: hw,
            info,
        }) = self.publish_screen_hardware(&local, preset).await
        {
            self.hub
                .set_screen_sink(Some((ScreenOutput::Feed(hw.feed()), width, height)));
            let maintenance = tokio::spawn(maintain_screen(ScreenJob {
                hub: self.hub.clone(),
                active: Arc::downgrade(&self.active),
                options: self.options,
                preset,
                // The GPU is never CPU-limited; hardware failures are handled separately.
                fallback: Vec::new(),
                track,
                info: info.clone(),
                hw: Some(hw.clone()),
                sid: publication.sid(),
            }));
            // The pre-encoded source lives in the encoder thread.
            let audio = if with_audio {
                publish_screen_audio(&local, &self.hub).await
            } else {
                None
            };
            active.screen = Some(ScreenTrack {
                publication,
                maintenance,
                info,
                hw: Some(hw),
                audio,
            });
            return Ok(());
        }

        tracing::info!("screen encoding in software");
        let source = NativeVideoSource::new(VideoResolution { width, height }, true);
        let codecs = software_screen_codecs(&self.options);
        let (publication, track, codec) =
            publish_screen_track(&local, &source, preset, &codecs, VideoEncoderBackend::Auto)
                .await?;
        self.hub
            .set_screen_sink(Some((ScreenOutput::Source(source), width, height)));
        let info = ScreenEncoderInfo::software(codec);
        let maintenance = tokio::spawn(maintain_screen(ScreenJob {
            hub: self.hub.clone(),
            active: Arc::downgrade(&self.active),
            options: self.options,
            preset,
            fallback: software_fallback(&self.options, codec, &codecs),
            track,
            info: info.clone(),
            hw: None,
            sid: publication.sid(),
        }));
        let audio = if with_audio {
            publish_screen_audio(&local, &self.hub).await
        } else {
            None
        };
        active.screen = Some(ScreenTrack {
            publication,
            maintenance,
            info,
            hw: None,
            audio,
        });
        Ok(())
    }

    async fn unpublish_screen(&self) -> Result<(), BoxError> {
        self.hub.set_screen_sink(None);
        let mut guard = self.active.lock().await;
        // A software republish holding the lock may have installed a new sink meanwhile.
        self.hub.set_screen_sink(None);
        let Some(active) = guard.as_mut() else {
            return Ok(());
        };
        if let Some(mut screen) = active.screen.take() {
            screen.maintenance.abort();
            let local = active.room.local_participant();
            screen.stop_audio(&self.hub, &local).await;
            local.unpublish_track(&screen.publication.sid()).await?;
        }
        Ok(())
    }
}

struct ScreenJob {
    hub: MediaHub,
    active: Weak<Mutex<Option<Active>>>,
    options: TransportOptions,
    preset: ScreenPreset,
    /// Codecs to switch to if the current encoder stays CPU-limited (empty: never switch).
    fallback: Vec<VideoCodec>,
    track: LocalVideoTrack,
    info: ScreenEncoderInfo,
    /// The GPU encoder, while the screen is hardware encoded.
    hw: Option<Arc<HwScreenEncoder>>,
    sid: TrackSid,
}

/// A screen track published as pre-encoded frames from the GPU encoder.
struct HwPublish {
    publication: LocalTrackPublication,
    track: LocalVideoTrack,
    encoder: Arc<HwScreenEncoder>,
    info: ScreenEncoderInfo,
}

/// Software screen codecs this host can encode, in preference order.
fn software_screen_codecs(options: &TransportOptions) -> Vec<VideoCodec> {
    let producible = LkRuntime::instance()
        .pc_factory()
        .video_encoder_codecs(VideoEncoderBackend::Auto);
    screen_codec_order(options.screen_codec, options.fallback_codec, &producible)
}

/// Codecs left to try if `codec` (published from `codecs`) stays CPU-limited.
fn software_fallback(
    options: &TransportOptions,
    codec: VideoCodec,
    codecs: &[VideoCodec],
) -> Vec<VideoCodec> {
    if codec == options.screen_codec {
        codecs[1..].to_vec()
    } else {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaintainAction {
    Continue,
    /// The GPU encoder died: republish the screen in software.
    FallBackToSoftware,
    /// The software encoder stays CPU-limited: republish with the fallback codec.
    SwitchCodec,
}

fn next_action(
    hw_failed: bool,
    cpu_limited_checks: u32,
    has_fallback: bool,
    hardware: bool,
) -> MaintainAction {
    if hw_failed {
        MaintainAction::FallBackToSoftware
    } else if SWITCH_ON_CPU_LIMIT
        && !hardware
        && has_fallback
        && cpu_limited_checks >= CPU_LIMITED_CHECKS
    {
        MaintainAction::SwitchCodec
    } else {
        MaintainAction::Continue
    }
}

/// Keeps the published screen healthy: repeats the last frame while capture is idle,
/// logs which encoder is really in use, republishes in software if the GPU encoder fails
/// and, if a software encoder stays CPU-limited on a host that may have hardware encoders,
/// republishes the screen with the fallback codec.
async fn maintain_screen(mut job: ScreenJob) {
    let mut tick = tokio::time::interval(SCREEN_REPEAT_IDLE);
    let mut next_stats = Instant::now() + SCREEN_STATS_EVERY;
    let mut first_report = true;
    let mut cpu_limited = 0;
    loop {
        tick.tick().await;
        job.hub.repeat_screen_if_idle(SCREEN_REPEAT_IDLE);
        if job.active.strong_count() == 0 {
            return;
        }
        if Instant::now() >= next_stats {
            next_stats = Instant::now() + SCREEN_STATS_EVERY;
            let limited = report_screen_encoder(&job.track, &job.info, first_report).await;
            first_report = false;
            cpu_limited = if limited { cpu_limited + 1 } else { 0 };
        }
        let hw_failed = job.hw.as_ref().is_some_and(|hw| hw.failed());
        let codecs = match next_action(
            hw_failed,
            cpu_limited,
            !job.fallback.is_empty(),
            job.info.hardware,
        ) {
            MaintainAction::Continue => continue,
            MaintainAction::FallBackToSoftware => {
                tracing::warn!(
                    codec = job.info.codec.as_str(),
                    encoder = %job.info.encoder,
                    "GPU screen encoder failed; switching to software"
                );
                software_screen_codecs(&job.options)
            }
            MaintainAction::SwitchCodec => {
                tracing::warn!(
                    from = job.info.codec.as_str(),
                    "screen encoder is CPU-limited; switching codec"
                );
                std::mem::take(&mut job.fallback)
            }
        };
        cpu_limited = 0;
        if !republish_screen_software(&mut job, &codecs).await {
            return;
        }
        first_report = true;
        next_stats = Instant::now() + SCREEN_STATS_EVERY;
    }
}

/// Logs the screen encoder stats; returns whether the encoder is CPU-limited.
async fn report_screen_encoder(
    track: &LocalVideoTrack,
    info: &ScreenEncoderInfo,
    first: bool,
) -> bool {
    let stats = match track.get_stats().await {
        Ok(stats) => stats,
        Err(e) => {
            tracing::debug!(error = %e, "screen encoder stats unavailable");
            return false;
        }
    };
    let mut limited = false;
    for s in stats {
        let RtcStats::OutboundRtp(rtp) = s else {
            continue;
        };
        let o = &rtp.outbound;
        let reason = o.quality_limitation_reason;
        limited |= reason == QualityLimitationReason::Cpu;
        macro_rules! report {
            ($level:ident) => {
                tracing::$level!(
                    codec = info.codec.as_str(),
                    gpu_encoder = info.hardware.then_some(info.encoder.as_str()),
                    encoder = %o.encoder_implementation,
                    hardware = info.hardware || o.power_efficient_encoder,
                    fps = o.frames_per_second,
                    width = o.frame_width,
                    height = o.frame_height,
                    limitation = ?reason,
                    key_frames = o.key_frames_encoded,
                    pli = o.pli_count,
                    nack = o.nack_count,
                    "screen encoder"
                )
            };
        }
        if reason == QualityLimitationReason::Cpu {
            report!(warn);
        } else if first {
            report!(info);
        } else {
            report!(debug);
        }
    }
    limited
}

/// Republishes the screen with libwebrtc's software encoder, using the first of `codecs`
/// LiveKit accepts. Returns `false` when the share is gone (or failed) and the maintenance
/// task should stop.
async fn republish_screen_software(job: &mut ScreenJob, codecs: &[VideoCodec]) -> bool {
    let Some(shared) = job.active.upgrade() else {
        return false;
    };
    let mut guard = shared.lock().await;
    let Some(active) = guard.as_mut() else {
        return false;
    };
    let Some(screen) = active.screen.as_mut() else {
        return false;
    };
    if screen.publication.sid() != job.sid {
        return false;
    }
    let local = active.room.local_participant();
    if let Err(e) = local.unpublish_track(&job.sid).await {
        tracing::warn!(error = %e, "failed to unpublish the screen track");
    }
    let (width, height) = (job.preset.width(), job.preset.height());
    let source = NativeVideoSource::new(VideoResolution { width, height }, true);
    match publish_screen_track(
        &local,
        &source,
        job.preset,
        codecs,
        VideoEncoderBackend::Auto,
    )
    .await
    {
        Ok((publication, track, codec)) => {
            // Replacing the sink drops the GPU feed; with `hw` gone its thread stops.
            job.hub
                .set_screen_sink(Some((ScreenOutput::Source(source), width, height)));
            tracing::info!(codec = codec.as_str(), "screen encoding in software");
            job.sid = publication.sid();
            job.track = track;
            job.info = ScreenEncoderInfo::software(codec);
            job.hw = None;
            job.fallback = software_fallback(&job.options, codec, codecs);
            screen.publication = publication;
            screen.info = job.info.clone();
            screen.hw = None;
            true
        }
        Err(e) => {
            tracing::error!(error = %e, "screen republish failed");
            job.hub.set_screen_sink(None);
            if let Some(mut dead) = active.screen.take() {
                dead.stop_audio(&job.hub, &local).await;
            }
            false
        }
    }
}

async fn pump_events(
    mut room_events: mpsc::UnboundedReceiver<RoomEvent>,
    tx: mpsc::Sender<TransportEvent>,
    hub: MediaHub,
    subscribe_video: bool,
) {
    let mut readers = JoinSet::new();
    let mut by_track: HashMap<TrackSid, AbortHandle> = HashMap::new();
    while let Some(event) = room_events.recv().await {
        let out = match event {
            RoomEvent::ParticipantConnected(p) | RoomEvent::ParticipantDisconnected(p)
                if is_viewer(&p) =>
            {
                None
            }
            RoomEvent::TrackPublished { participant, .. } if is_viewer(&participant) => None,
            RoomEvent::ParticipantConnected(p) => {
                Some(TransportEvent::ParticipantJoined(describe(&p)))
            }
            RoomEvent::ParticipantDisconnected(p) => {
                hub.remove_remote(&identity(&p));
                Some(TransportEvent::ParticipantLeft { id: identity(&p) })
            }
            RoomEvent::TrackPublished {
                publication,
                participant,
            } => {
                if !subscribe_video {
                    subscribe_wanted(&publication, false);
                }
                match publication.source() {
                    TrackSource::Microphone => Some(TransportEvent::MuteChanged {
                        id: identity(&participant),
                        muted: !has_source(&participant, TrackSource::Microphone, true),
                    }),
                    TrackSource::Screenshare => Some(TransportEvent::ScreenShareChanged {
                        id: identity(&participant),
                        sharing: true,
                    }),
                    _ => None,
                }
            }
            RoomEvent::TrackUnpublished {
                publication,
                participant,
            } => match publication.source() {
                TrackSource::Microphone => Some(TransportEvent::MuteChanged {
                    id: identity(&participant),
                    muted: true,
                }),
                TrackSource::Screenshare => Some(TransportEvent::ScreenShareChanged {
                    id: identity(&participant),
                    sharing: false,
                }),
                _ => None,
            },
            RoomEvent::TrackMuted {
                participant: Participant::Remote(p),
                publication,
            }
            | RoomEvent::TrackUnmuted {
                participant: Participant::Remote(p),
                publication,
            } if publication.source() == TrackSource::Microphone => {
                Some(TransportEvent::MuteChanged {
                    id: identity(&p),
                    muted: !has_source(&p, TrackSource::Microphone, true),
                })
            }
            RoomEvent::ActiveSpeakersChanged { speakers } => Some(TransportEvent::ActiveSpeakers(
                speakers.iter().map(|p| p.identity().to_string()).collect(),
            )),
            RoomEvent::TrackSubscribed {
                track,
                publication,
                participant,
            } => {
                let sid = track.sid();
                let handle = match track {
                    RemoteTrack::Audio(audio) => readers.spawn(read_audio(
                        NativeAudioStream::new(audio.rtc_track(), SAMPLE_RATE as i32, 1),
                        audio_mixer_key(&identity(&participant), publication.source()),
                        hub.clone(),
                    )),
                    RemoteTrack::Video(video) => readers.spawn(read_video(
                        NativeVideoStream::new(video.rtc_track()),
                        identity(&participant),
                        hub.clone(),
                    )),
                };
                if let Some(old) = by_track.insert(sid, handle) {
                    old.abort();
                }
                None
            }
            RoomEvent::TrackUnsubscribed { track, .. } => {
                if let Some(handle) = by_track.remove(&track.sid()) {
                    handle.abort();
                }
                None
            }
            RoomEvent::Disconnected { reason } => {
                let _ = tx
                    .send(TransportEvent::Disconnected {
                        reason: format!("{reason:?}"),
                    })
                    .await;
                break;
            }
            _ => None,
        };
        if let Some(out) = out {
            if tx.send(out).await.is_err() {
                break;
            }
        }
        while readers.try_join_next().is_some() {}
    }
}

async fn read_audio(mut stream: NativeAudioStream, mixer_key: String, hub: MediaHub) {
    while let Some(frame) = stream.next().await {
        hub.push_remote_audio(&mixer_key, &frame.data);
    }
}

/// Mixer channel of a remote audio track: screen audio is kept apart from the voice.
fn audio_mixer_key(identity: &str, source: TrackSource) -> String {
    if source == TrackSource::ScreenshareAudio {
        screen_audio_key(identity)
    } else {
        identity.to_owned()
    }
}

async fn read_video(mut stream: NativeVideoStream, participant: String, hub: MediaHub) {
    while let Some(frame) = stream.next().await {
        hub.count_remote_video();
        if hub.preview_due(&participant) {
            let mut i420 = frame.buffer.to_i420();
            hub.emit_preview(&participant, &mut i420);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_mixer_key_separates_screen_audio() {
        assert_eq!(audio_mixer_key("bob", TrackSource::Microphone), "bob");
        assert_eq!(
            audio_mixer_key("bob", TrackSource::ScreenshareAudio),
            "bob#screen"
        );
    }

    #[test]
    fn maintenance_decisions() {
        use MaintainAction::*;
        assert_eq!(next_action(true, 0, false, true), FallBackToSoftware);
        // The GPU never switches codec for being CPU-limited.
        assert_eq!(next_action(false, CPU_LIMITED_CHECKS, true, true), Continue);
        assert_eq!(
            next_action(false, CPU_LIMITED_CHECKS, true, false),
            if SWITCH_ON_CPU_LIMIT {
                SwitchCodec
            } else {
                Continue
            }
        );
        assert_eq!(
            next_action(false, CPU_LIMITED_CHECKS, false, false),
            Continue
        );
        assert_eq!(next_action(false, 0, true, false), Continue);
    }

    #[test]
    fn hardware_candidates_keep_preference_order() {
        let on = TransportOptions::default();
        assert_eq!(
            hardware_candidates(&on),
            vec![VideoCodec::AV1, VideoCodec::H264]
        );
        let vp8_first = TransportOptions {
            screen_codec: VideoCodec::VP8,
            ..on
        };
        assert_eq!(hardware_candidates(&vp8_first), vec![VideoCodec::H264]);
        let off = TransportOptions {
            hardware_encoder: false,
            ..on
        };
        assert!(hardware_candidates(&off).is_empty());
    }

    #[test]
    fn screen_codecs_fall_back_to_h264_when_av1_cannot_be_encoded() {
        let all = [EncoderCodec::AV1, EncoderCodec::H264, EncoderCodec::VP8];
        assert_eq!(
            screen_codec_order(VideoCodec::AV1, VideoCodec::H264, &all),
            [VideoCodec::AV1, VideoCodec::H264, VideoCodec::VP8]
        );
        assert_eq!(
            screen_codec_order(VideoCodec::AV1, VideoCodec::H264, &[EncoderCodec::H264]),
            [VideoCodec::H264, VideoCodec::VP8]
        );
        assert_eq!(
            screen_codec_order(VideoCodec::VP8, VideoCodec::VP8, &[]),
            [VideoCodec::VP8]
        );
    }
}
