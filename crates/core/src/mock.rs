//! In-memory implementations of the core traits for tests and UI development.

use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::traits::{
    AudioSource, BoxError, ScreenSource, Session, TokenProvider, Transport, TransportEvent,
};
use crate::types::{
    AudioDevices, Credentials, Device, DeviceKind, RemoteParticipant, ScreenPreset,
    ScreenSourceInfo, ScreenSourceKind,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Default)]
struct TokenState {
    error: Option<String>,
    calls: u32,
}

#[derive(Debug, Default)]
pub struct MockTokenProvider {
    state: Mutex<TokenState>,
}

impl MockTokenProvider {
    /// Makes every following request fail with `message` (`None` restores success).
    pub fn set_error(&self, message: Option<&str>) {
        lock(&self.state).error = message.map(str::to_owned);
    }

    pub fn calls(&self) -> u32 {
        lock(&self.state).calls
    }
}

#[async_trait]
impl TokenProvider for MockTokenProvider {
    async fn fetch_token(
        &self,
        _server_url: &str,
        room: &str,
        name: &str,
    ) -> Result<Credentials, BoxError> {
        let mut state = lock(&self.state);
        state.calls += 1;
        if let Some(err) = &state.error {
            return Err(err.clone().into());
        }
        Ok(Credentials {
            token: format!("mock-token-{}-{room}-{name}", state.calls),
            url: "ws://mock".to_owned(),
        })
    }
}

#[derive(Debug, Default)]
struct TransportState {
    fail_connects: u32,
    connect_calls: u32,
    disconnect_calls: u32,
    last_token: Option<String>,
    initial_participants: Vec<RemoteParticipant>,
    sender: Option<mpsc::Sender<TransportEvent>>,
    mic_muted: Option<bool>,
    screen: Option<ScreenPreset>,
}

#[derive(Debug, Default)]
pub struct MockTransport {
    state: Mutex<TransportState>,
}

impl MockTransport {
    pub fn fail_next_connects(&self, count: u32) {
        lock(&self.state).fail_connects = count;
    }

    pub fn set_initial_participants(&self, participants: Vec<RemoteParticipant>) {
        lock(&self.state).initial_participants = participants;
    }

    /// Pushes an event into the current session. Returns `false` if there is none.
    pub async fn emit(&self, event: TransportEvent) -> bool {
        let sender = lock(&self.state).sender.clone();
        match sender {
            Some(tx) => tx.send(event).await.is_ok(),
            None => false,
        }
    }

    /// Simulates a network drop: emits `Disconnected` and closes the session.
    pub async fn drop_connection(&self, reason: &str) {
        let sender = lock(&self.state).sender.take();
        if let Some(tx) = sender {
            let _ = tx
                .send(TransportEvent::Disconnected {
                    reason: reason.to_owned(),
                })
                .await;
        }
    }

    /// Closes the session's event channel without sending `Disconnected`.
    pub fn close_session(&self) {
        lock(&self.state).sender = None;
    }

    pub fn connect_calls(&self) -> u32 {
        lock(&self.state).connect_calls
    }

    pub fn disconnect_calls(&self) -> u32 {
        lock(&self.state).disconnect_calls
    }

    pub fn last_token(&self) -> Option<String> {
        lock(&self.state).last_token.clone()
    }

    pub fn mic_muted(&self) -> Option<bool> {
        lock(&self.state).mic_muted
    }

    pub fn published_screen(&self) -> Option<ScreenPreset> {
        lock(&self.state).screen
    }

    pub fn is_connected(&self) -> bool {
        lock(&self.state).sender.is_some()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn connect(&self, _url: &str, token: &str) -> Result<Session, BoxError> {
        let mut state = lock(&self.state);
        state.connect_calls += 1;
        state.last_token = Some(token.to_owned());
        if state.fail_connects > 0 {
            state.fail_connects -= 1;
            return Err("mock connect failure".into());
        }
        let (tx, rx) = mpsc::channel(64);
        state.sender = Some(tx);
        Ok(Session {
            local_identity: format!("local-{}", state.connect_calls),
            participants: state.initial_participants.clone(),
            events: rx,
        })
    }

    async fn disconnect(&self) {
        let mut state = lock(&self.state);
        state.disconnect_calls += 1;
        state.sender = None;
        state.screen = None;
    }

    async fn set_microphone_muted(&self, muted: bool) -> Result<(), BoxError> {
        lock(&self.state).mic_muted = Some(muted);
        Ok(())
    }

    async fn publish_screen(&self, preset: ScreenPreset) -> Result<(), BoxError> {
        lock(&self.state).screen = Some(preset);
        Ok(())
    }

    async fn unpublish_screen(&self) -> Result<(), BoxError> {
        lock(&self.state).screen = None;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MockAudioState {
    pub started: bool,
    pub fail_start: bool,
    pub capture_muted: bool,
    pub playback_muted: bool,
    pub input: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Default)]
pub struct MockAudio {
    state: Mutex<MockAudioState>,
}

impl MockAudio {
    pub fn state(&self) -> MockAudioState {
        lock(&self.state).clone()
    }

    pub fn set_fail_start(&self, fail: bool) {
        lock(&self.state).fail_start = fail;
    }
}

#[async_trait]
impl AudioSource for MockAudio {
    async fn list_devices(&self) -> Result<AudioDevices, BoxError> {
        let device = |id: &str, name: &str| Device {
            id: id.to_owned(),
            name: name.to_owned(),
        };
        Ok(AudioDevices {
            inputs: vec![device("mic-1", "Mock Mic")],
            outputs: vec![device("spk-1", "Mock Speakers")],
        })
    }

    async fn select_device(&self, kind: DeviceKind, id: &str) -> Result<(), BoxError> {
        let devices = self.list_devices().await?;
        let list = match kind {
            DeviceKind::Input => &devices.inputs,
            DeviceKind::Output => &devices.outputs,
        };
        if !list.iter().any(|d| d.id == id) {
            return Err(format!("unknown device {id}").into());
        }
        let mut state = lock(&self.state);
        match kind {
            DeviceKind::Input => state.input = Some(id.to_owned()),
            DeviceKind::Output => state.output = Some(id.to_owned()),
        }
        Ok(())
    }

    async fn start(&self) -> Result<(), BoxError> {
        let mut state = lock(&self.state);
        if state.fail_start {
            return Err("mock audio failure".into());
        }
        state.started = true;
        Ok(())
    }

    async fn stop(&self) {
        lock(&self.state).started = false;
    }

    async fn set_capture_muted(&self, muted: bool) -> Result<(), BoxError> {
        lock(&self.state).capture_muted = muted;
        Ok(())
    }

    async fn set_playback_muted(&self, muted: bool) -> Result<(), BoxError> {
        lock(&self.state).playback_muted = muted;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct MockScreen {
    active: Mutex<Option<(String, ScreenPreset)>>,
}

impl MockScreen {
    pub fn active(&self) -> Option<(String, ScreenPreset)> {
        lock(&self.active).clone()
    }
}

#[async_trait]
impl ScreenSource for MockScreen {
    async fn list_sources(&self) -> Result<Vec<ScreenSourceInfo>, BoxError> {
        Ok(vec![
            ScreenSourceInfo {
                id: "monitor-0".to_owned(),
                name: "Monitor 1".to_owned(),
                kind: ScreenSourceKind::Monitor,
            },
            ScreenSourceInfo {
                id: "window-42".to_owned(),
                name: "Game".to_owned(),
                kind: ScreenSourceKind::Window,
            },
        ])
    }

    async fn start(&self, source_id: &str, preset: ScreenPreset) -> Result<(), BoxError> {
        let sources = self.list_sources().await?;
        if !sources.iter().any(|s| s.id == source_id) {
            return Err(format!("unknown source {source_id}").into());
        }
        *lock(&self.active) = Some((source_id.to_owned(), preset));
        Ok(())
    }

    async fn stop(&self) {
        *lock(&self.active) = None;
    }
}
