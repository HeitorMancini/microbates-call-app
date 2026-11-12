//! Drives real LiveKit clients (Rust SDK, libwebrtc) for the load test.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use futures_util::future::join_all;
use futures_util::StreamExt;
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::prelude::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::stats::RtcStats;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token::{AccessToken, VideoGrants};

use crate::impairment::{Impairment, ImpairmentHook};
use crate::metrics::{
    evaluate, summarize, InboundSample, LoadReport, PublisherMetrics, Thresholds, ViewerMetrics,
};
use crate::pattern::{PlanesMut, TestPattern};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Codec {
    #[default]
    Vp8,
    H264,
    Vp9,
    Av1,
}

impl FromStr for Codec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "vp8" => Ok(Self::Vp8),
            "h264" => Ok(Self::H264),
            "vp9" => Ok(Self::Vp9),
            "av1" => Ok(Self::Av1),
            other => Err(format!(
                "codec desconhecido: {other} (use vp8, h264, vp9, av1)"
            )),
        }
    }
}

impl From<Codec> for VideoCodec {
    fn from(c: Codec) -> Self {
        match c {
            Codec::Vp8 => VideoCodec::VP8,
            Codec::H264 => VideoCodec::H264,
            Codec::Vp9 => VideoCodec::VP9,
            Codec::Av1 => VideoCodec::AV1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub room: String,
    /// Total participants including the publisher (>= 2).
    pub participants: usize,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub codec: Codec,
    pub warmup: Duration,
    pub duration: Duration,
    pub connect_timeout: Duration,
    pub subscribe_timeout: Duration,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:7880".into(),
            api_key: "devkey".into(),
            api_secret: "devsecretdevsecretdevsecretdevsecret".into(),
            room: "load-test".into(),
            participants: 7,
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 3000,
            codec: Codec::Vp8,
            warmup: Duration::from_secs(5),
            duration: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(15),
            subscribe_timeout: Duration::from_secs(20),
        }
    }
}

impl RunConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.participants < 2 {
            bail!("participants deve ser >= 2 (1 publicador + espectadores)");
        }
        if self.width < 16
            || self.height < 16
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
        {
            bail!("resolução deve ser par e >= 16x16");
        }
        if self.fps == 0 || self.bitrate_kbps == 0 || self.duration.is_zero() {
            bail!("fps, bitrate e duração devem ser > 0");
        }
        Ok(())
    }

    fn token(&self, identity: &str) -> anyhow::Result<String> {
        AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(identity)
            .with_name(identity)
            .with_grants(VideoGrants {
                room_join: true,
                room: self.room.clone(),
                ..Default::default()
            })
            .to_jwt()
            .context("falha ao gerar token")
    }
}

struct Client {
    identity: String,
    room: Room,
    connect_ms: f64,
    connected_at: Instant,
}

async fn connect(cfg: &RunConfig, identity: String) -> anyhow::Result<(Client, RoomEvents)> {
    let token = cfg.token(&identity)?;
    let started = Instant::now();
    let (room, events) = tokio::time::timeout(
        cfg.connect_timeout,
        Room::connect(&cfg.url, &token, RoomOptions::default()),
    )
    .await
    .with_context(|| format!("{identity}: timeout ao conectar"))?
    .with_context(|| format!("{identity}: falha ao conectar em {}", cfg.url))?;
    let connected_at = Instant::now();
    Ok((
        Client {
            identity,
            room,
            connect_ms: (connected_at - started).as_secs_f64() * 1000.0,
            connected_at,
        },
        events,
    ))
}

type RoomEvents = tokio::sync::mpsc::UnboundedReceiver<RoomEvent>;

#[derive(Default)]
struct ViewerState {
    track: Mutex<Option<RemoteVideoTrack>>,
    frames: AtomicU64,
    first_frame: Mutex<Option<Instant>>,
}

fn spawn_viewer_loop(mut events: RoomEvents, state: Arc<ViewerState>) {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let RoomEvent::TrackSubscribed {
                track: RemoteTrack::Video(track),
                ..
            } = event
            else {
                continue;
            };
            *state.track.lock().unwrap() = Some(track.clone());
            let state = state.clone();
            tokio::spawn(async move {
                let mut stream = NativeVideoStream::new(track.rtc_track());
                while stream.next().await.is_some() {
                    if state.frames.fetch_add(1, Ordering::Relaxed) == 0 {
                        *state.first_frame.lock().unwrap() = Some(Instant::now());
                    }
                }
            });
        }
    });
}

fn drain(mut events: RoomEvents) {
    tokio::spawn(async move { while events.recv().await.is_some() {} });
}

async fn sample_viewer(state: &ViewerState, origin: Instant) -> Option<InboundSample> {
    let track = state.track.lock().unwrap().clone()?;
    let stats = track.get_stats().await.ok()?;
    let at_secs = origin.elapsed().as_secs_f64();
    let frames_received = state.frames.load(Ordering::Relaxed);
    stats
        .into_iter()
        .filter_map(|s| match s {
            RtcStats::InboundRtp(s) if s.stream.kind == "video" => Some(s),
            _ => None,
        })
        .max_by_key(|s| s.inbound.bytes_received)
        .map(|s| InboundSample {
            at_secs,
            bytes_received: s.inbound.bytes_received,
            packets_received: s.received.packets_received,
            packets_lost: s.received.packets_lost,
            frames_received,
            jitter_secs: s.received.jitter,
            frame_width: s.inbound.frame_width,
            frame_height: s.inbound.frame_height,
        })
}

async fn bytes_sent(track: &LocalVideoTrack) -> u64 {
    track
        .get_stats()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| match s {
            RtcStats::OutboundRtp(s) if s.stream.kind == "video" => Some(s.sent.bytes_sent),
            _ => None,
        })
        .sum()
}

/// Feeds the synthetic pattern into `source` at `fps` on a dedicated thread.
struct FramePump {
    stop: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FramePump {
    fn start(source: NativeVideoSource, width: u32, height: u32, fps: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(AtomicU64::new(0));
        let (stop2, frames2) = (stop.clone(), frames.clone());
        let thread = std::thread::spawn(move || {
            let pattern = TestPattern::new(width, height);
            let interval = Duration::from_secs_f64(1.0 / f64::from(fps));
            let start = Instant::now();
            let mut n: u64 = 0;
            while !stop2.load(Ordering::Relaxed) {
                let mut buffer = I420Buffer::new(width, height);
                let (sy, su, sv) = buffer.strides();
                let (y, u, v) = buffer.data_mut();
                pattern.render(
                    n,
                    PlanesMut {
                        y,
                        stride_y: sy as usize,
                        u,
                        stride_u: su as usize,
                        v,
                        stride_v: sv as usize,
                    },
                );
                let frame = VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us: start.elapsed().as_micros() as i64,
                    frame_metadata: None,
                    buffer,
                };
                source.capture_frame(&frame);
                frames2.fetch_add(1, Ordering::Relaxed);
                n += 1;
                let next = start + interval.mul_f64(n as f64);
                if let Some(wait) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        });
        Self {
            stop,
            frames,
            thread: Some(thread),
        }
    }
}

impl Drop for FramePump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Runs one load test and evaluates it. `impairment` is applied through `hook`
/// once every viewer is subscribed, and cleared before returning.
pub async fn run(
    cfg: &RunConfig,
    thresholds: &Thresholds,
    impairment: Option<Impairment>,
    hook: &dyn ImpairmentHook,
) -> anyhow::Result<LoadReport> {
    cfg.validate()?;
    let origin = Instant::now();

    let identities = (0..cfg.participants).map(|i| {
        if i == 0 {
            "publisher".to_owned()
        } else {
            format!("viewer-{i}")
        }
    });
    let connected = join_all(identities.map(|id| connect(cfg, id))).await;
    let mut clients = Vec::with_capacity(connected.len());
    let mut viewers = Vec::new();
    for (i, res) in connected.into_iter().enumerate() {
        let (client, events) = res?;
        if i == 0 {
            drain(events);
        } else {
            let state = Arc::new(ViewerState::default());
            spawn_viewer_loop(events, state.clone());
            viewers.push(state);
        }
        clients.push(client);
    }
    tracing::info!("{} clientes conectados", clients.len());

    let source = NativeVideoSource::new(
        VideoResolution {
            width: cfg.width,
            height: cfg.height,
        },
        true,
    );
    let track =
        LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(source.clone()));
    let pump = FramePump::start(source, cfg.width, cfg.height, cfg.fps);
    clients[0]
        .room
        .local_participant()
        .publish_track(
            LocalTrack::Video(track.clone()),
            TrackPublishOptions {
                source: TrackSource::Screenshare,
                video_codec: cfg.codec.into(),
                video_encoding: Some(VideoEncoding {
                    max_bitrate: u64::from(cfg.bitrate_kbps) * 1000,
                    max_framerate: f64::from(cfg.fps),
                }),
                simulcast: false,
                ..Default::default()
            },
        )
        .await
        .context("falha ao publicar a tela sintética")?;

    let deadline = Instant::now() + cfg.subscribe_timeout;
    while viewers
        .iter()
        .any(|v| v.frames.load(Ordering::Relaxed) == 0)
        && Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let impairment = impairment.filter(|i| !i.is_none());
    if let Some(imp) = &impairment {
        hook.apply(imp)
            .context("gancho de impairment (apply) falhou")?;
    }

    tokio::time::sleep(cfg.warmup).await;
    let sent_start = (origin.elapsed().as_secs_f64(), bytes_sent(&track).await);
    let starts = join_all(viewers.iter().map(|v| sample_viewer(v, origin))).await;
    tokio::time::sleep(cfg.duration).await;
    let ends = join_all(viewers.iter().map(|v| sample_viewer(v, origin))).await;
    let sent_end = (origin.elapsed().as_secs_f64(), bytes_sent(&track).await);

    if impairment.is_some() {
        if let Err(e) = hook.clear() {
            tracing::warn!("gancho de impairment (clear) falhou: {e:#}");
        }
    }
    let frames_captured = pump.frames.load(Ordering::Relaxed);
    drop(pump);

    let viewer_metrics: Vec<ViewerMetrics> = viewers
        .iter()
        .zip(&clients[1..])
        .zip(starts.iter().zip(&ends))
        .map(|((state, client), samples)| {
            let first_frame_ms = state.first_frame.lock().unwrap().map(|t| {
                t.saturating_duration_since(client.connected_at)
                    .as_secs_f64()
                    * 1000.0
            });
            match samples {
                (Some(a), Some(b)) => ViewerMetrics::from_window(
                    client.identity.clone(),
                    client.connect_ms,
                    first_frame_ms,
                    a,
                    b,
                ),
                _ => ViewerMetrics::unsubscribed(client.identity.clone(), client.connect_ms),
            }
        })
        .collect();

    let window = sent_end.0 - sent_start.0;
    let publisher = PublisherMetrics {
        identity: clients[0].identity.clone(),
        connect_ms: clients[0].connect_ms,
        target_bitrate_kbps: f64::from(cfg.bitrate_kbps),
        sent_bitrate_kbps: if window > 0.0 {
            sent_end.1.saturating_sub(sent_start.1) as f64 * 8.0 / 1000.0 / window
        } else {
            0.0
        },
        frames_captured,
    };

    for c in &clients {
        let _ = c.room.close().await;
    }

    let summary = summarize(&viewer_metrics, publisher.connect_ms);
    let violations = evaluate(&summary, thresholds);
    Ok(LoadReport {
        room: cfg.room.clone(),
        participants: cfg.participants,
        width: cfg.width,
        height: cfg.height,
        fps: cfg.fps,
        measured_secs: cfg.duration.as_secs_f64(),
        impairment,
        publisher,
        viewers: viewer_metrics,
        summary,
        thresholds: thresholds.clone(),
        passed: violations.is_empty(),
        violations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_parsing() {
        assert_eq!("H264".parse::<Codec>(), Ok(Codec::H264));
        assert!("mpeg2".parse::<Codec>().is_err());
    }

    #[test]
    fn config_validation() {
        assert!(RunConfig::default().validate().is_ok());
        for bad in [
            RunConfig {
                participants: 1,
                ..Default::default()
            },
            RunConfig {
                width: 1919,
                ..Default::default()
            },
            RunConfig {
                fps: 0,
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }
}
