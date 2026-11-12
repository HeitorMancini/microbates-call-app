//! [`Transport`] backed by the LiveKit Rust SDK (libwebrtc).

use std::collections::HashMap;

use async_trait::async_trait;
use futures_util::StreamExt;
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::AudioFrame;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use sheeshcord_core::{
    BoxError, RemoteParticipant as CoreParticipant, ScreenPreset, Session, Transport,
    TransportEvent,
};
use tokio::sync::{mpsc, Mutex};
use tokio::task::{AbortHandle, JoinHandle, JoinSet};

use crate::hub::{MediaHub, SAMPLE_RATE};

/// Encoder settings for the screen-share track.
///
/// VP8 is the default because every libwebrtc build ships a software encoder for it;
/// H264 may pick a hardware encoder (`VideoEncoderBackend::Auto`) where available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportOptions {
    pub screen_codec: VideoCodec,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            screen_codec: VideoCodec::VP8,
        }
    }
}

/// Upper bound for the screen-share bitrate of each preset (bits per second).
pub fn preset_max_bitrate(preset: ScreenPreset) -> u64 {
    match preset {
        ScreenPreset::P1080Fps60 => 6_000_000,
        ScreenPreset::P1080Fps30 => 4_000_000,
        ScreenPreset::P720Fps30 => 2_500_000,
    }
}

struct ScreenTrack {
    publication: LocalTrackPublication,
}

struct Active {
    room: Room,
    mic: LocalTrackPublication,
    screen: Option<ScreenTrack>,
    events: JoinHandle<()>,
    mic_pump: JoinHandle<()>,
}

/// LiveKit session: publishes the microphone (fed by [`MediaHub::push_mic`]) and, on
/// demand, the screen (fed by [`MediaHub::push_screen_bgra`]); remote audio/video is
/// delivered back into the hub.
pub struct LiveKitTransport {
    hub: MediaHub,
    options: TransportOptions,
    active: Mutex<Option<Active>>,
}

impl LiveKitTransport {
    pub fn new(hub: MediaHub) -> Self {
        Self::with_options(hub, TransportOptions::default())
    }

    pub fn with_options(hub: MediaHub, options: TransportOptions) -> Self {
        Self {
            hub,
            options,
            active: Mutex::new(None),
        }
    }

    pub fn hub(&self) -> &MediaHub {
        &self.hub
    }

    async fn close(&self, active: Active) {
        active.events.abort();
        active.mic_pump.abort();
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
        if let Some(active) = self.active.get_mut().take() {
            active.events.abort();
            active.mic_pump.abort();
        }
    }
}

fn identity(p: &RemoteParticipant) -> String {
    p.identity().to_string()
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
        options.auto_subscribe = true;
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

        let participants = room.remote_participants().values().map(describe).collect();
        let (tx, rx) = mpsc::channel(256);
        let events = tokio::spawn(pump_events(room_events, tx, self.hub.clone()));

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

    async fn publish_screen(&self, preset: ScreenPreset) -> Result<(), BoxError> {
        let mut guard = self.active.lock().await;
        let active = guard.as_mut().ok_or("not connected")?;
        if let Some(screen) = active.screen.take() {
            self.hub.set_screen_sink(None);
            let _ = active
                .room
                .local_participant()
                .unpublish_track(&screen.publication.sid())
                .await;
        }

        let (width, height) = (preset.width(), preset.height());
        let source = NativeVideoSource::new(VideoResolution { width, height }, true);
        let track =
            LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(source.clone()));
        let publication = active
            .room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track),
                TrackPublishOptions {
                    source: TrackSource::Screenshare,
                    video_codec: self.options.screen_codec,
                    simulcast: false,
                    video_encoding: Some(VideoEncoding {
                        max_bitrate: preset_max_bitrate(preset),
                        max_framerate: f64::from(preset.fps()),
                    }),
                    ..Default::default()
                },
            )
            .await?;
        self.hub.set_screen_sink(Some((source, width, height)));
        active.screen = Some(ScreenTrack { publication });
        Ok(())
    }

    async fn unpublish_screen(&self) -> Result<(), BoxError> {
        self.hub.set_screen_sink(None);
        let mut guard = self.active.lock().await;
        let Some(active) = guard.as_mut() else {
            return Ok(());
        };
        if let Some(screen) = active.screen.take() {
            active
                .room
                .local_participant()
                .unpublish_track(&screen.publication.sid())
                .await?;
        }
        Ok(())
    }
}

async fn pump_events(
    mut room_events: mpsc::UnboundedReceiver<RoomEvent>,
    tx: mpsc::Sender<TransportEvent>,
    hub: MediaHub,
) {
    let mut readers = JoinSet::new();
    let mut by_track: HashMap<TrackSid, AbortHandle> = HashMap::new();
    while let Some(event) = room_events.recv().await {
        let out = match event {
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
            } => match publication.source() {
                TrackSource::Microphone => Some(TransportEvent::MuteChanged {
                    id: identity(&participant),
                    muted: !has_source(&participant, TrackSource::Microphone, true),
                }),
                TrackSource::Screenshare => Some(TransportEvent::ScreenShareChanged {
                    id: identity(&participant),
                    sharing: true,
                }),
                _ => None,
            },
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
                track, participant, ..
            } => {
                let sid = track.sid();
                let handle = match track {
                    RemoteTrack::Audio(audio) => readers.spawn(read_audio(
                        NativeAudioStream::new(audio.rtc_track(), SAMPLE_RATE as i32, 1),
                        identity(&participant),
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

async fn read_audio(mut stream: NativeAudioStream, participant: String, hub: MediaHub) {
    while let Some(frame) = stream.next().await {
        hub.push_remote_audio(&participant, &frame.data);
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
