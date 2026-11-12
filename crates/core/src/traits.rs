use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::{
    AudioDevices, Credentials, DeviceKind, JoinParams, RemoteParticipant, ScreenPreset,
    ScreenSourceInfo,
};

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Obtains LiveKit credentials (normally via `POST {server_url}/token`).
#[async_trait]
pub trait TokenProvider: Send + Sync {
    async fn fetch_token(&self, params: &JoinParams) -> Result<Credentials, BoxError>;
}

/// Event pushed by a live [`Transport`] session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    ParticipantJoined(RemoteParticipant),
    ParticipantLeft {
        id: String,
    },
    MuteChanged {
        id: String,
        muted: bool,
    },
    ScreenShareChanged {
        id: String,
        sharing: bool,
    },
    /// Full list of participants currently speaking (may include the local identity).
    ActiveSpeakers(Vec<String>),
    /// Unexpected loss of connection; the room will try to reconnect.
    Disconnected {
        reason: String,
    },
}

pub struct Session {
    pub local_identity: String,
    pub participants: Vec<RemoteParticipant>,
    /// Closing this channel is treated like [`TransportEvent::Disconnected`].
    pub events: mpsc::Receiver<TransportEvent>,
}

/// Real-time media connection (LiveKit in production).
#[async_trait]
pub trait Transport: Send + Sync {
    async fn connect(&self, url: &str, token: &str) -> Result<Session, BoxError>;
    async fn disconnect(&self);
    async fn set_microphone_muted(&self, muted: bool) -> Result<(), BoxError>;
    async fn publish_screen(&self, preset: ScreenPreset) -> Result<(), BoxError>;
    async fn unpublish_screen(&self) -> Result<(), BoxError>;
}

/// Microphone capture and speaker playback.
#[async_trait]
pub trait AudioSource: Send + Sync {
    async fn list_devices(&self) -> Result<AudioDevices, BoxError>;
    async fn select_device(&self, kind: DeviceKind, id: &str) -> Result<(), BoxError>;
    async fn start(&self) -> Result<(), BoxError>;
    async fn stop(&self);
    async fn set_capture_muted(&self, muted: bool) -> Result<(), BoxError>;
    async fn set_playback_muted(&self, muted: bool) -> Result<(), BoxError>;
}

/// Screen/window capture.
#[async_trait]
pub trait ScreenSource: Send + Sync {
    async fn list_sources(&self) -> Result<Vec<ScreenSourceInfo>, BoxError>;
    async fn start(&self, source_id: &str, preset: ScreenPreset) -> Result<(), BoxError>;
    async fn stop(&self);
}
