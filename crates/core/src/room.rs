use std::sync::{Arc, Mutex, PoisonError};

use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;

use crate::backoff::Backoff;
use crate::traits::{
    AudioSource, BoxError, ScreenSource, TokenProvider, Transport, TransportEvent,
};
use crate::types::{
    AudioDevices, DeviceKind, JoinParams, Participant, RoomEvent, RoomState, RoomStatus,
    ScreenPreset, ScreenSourceInfo, Snapshot,
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("not connected to a room")]
    NotConnected,
    #[error("already in a room")]
    AlreadyActive,
    #[error("token error: {0}")]
    Token(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("screen error: {0}")]
    Screen(String),
    #[error("room controller has stopped")]
    Closed,
}

impl CoreError {
    fn token(e: BoxError) -> Self {
        Self::Token(e.to_string())
    }
    fn transport(e: BoxError) -> Self {
        Self::Transport(e.to_string())
    }
    fn audio(e: BoxError) -> Self {
        Self::Audio(e.to_string())
    }
    fn screen(e: BoxError) -> Self {
        Self::Screen(e.to_string())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Clone)]
pub struct Dependencies {
    pub tokens: Arc<dyn TokenProvider>,
    pub transport: Arc<dyn Transport>,
    pub audio: Arc<dyn AudioSource>,
    pub screen: Arc<dyn ScreenSource>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RoomConfig {
    pub backoff: Backoff,
}

type Reply = oneshot::Sender<CoreResult<()>>;

enum Command {
    Join(JoinParams, Reply),
    Leave(Reply),
    SetMuted(bool, Reply),
    SetDeafened(bool, Reply),
    StartScreenShare(String, ScreenPreset, Reply),
    StopScreenShare(Reply),
}

/// Cloneable handle to the room state machine. Must be created inside a Tokio runtime.
#[derive(Clone)]
pub struct RoomController {
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<RoomEvent>,
    snapshot: Arc<Mutex<Snapshot>>,
    deps: Dependencies,
}

impl RoomController {
    pub fn new(deps: Dependencies, config: RoomConfig) -> Self {
        let (commands, rx) = mpsc::channel(32);
        let (events, _) = broadcast::channel(256);
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let actor = Actor {
            deps: deps.clone(),
            config,
            events: events.clone(),
            snapshot: Arc::clone(&snapshot),
            state: RoomState::default(),
            participants: Vec::new(),
            muted: false,
            deafened: false,
            sharing: None,
            params: None,
            session_events: None,
            retry: None,
        };
        tokio::spawn(actor.run(rx));
        Self {
            commands,
            events,
            snapshot,
            deps,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RoomEvent> {
        self.events.subscribe()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub async fn join(&self, params: JoinParams) -> CoreResult<()> {
        self.request(|reply| Command::Join(params, reply)).await
    }

    pub async fn leave(&self) -> CoreResult<()> {
        self.request(Command::Leave).await
    }

    pub async fn set_muted(&self, muted: bool) -> CoreResult<()> {
        self.request(|reply| Command::SetMuted(muted, reply)).await
    }

    pub async fn set_deafened(&self, deafened: bool) -> CoreResult<()> {
        self.request(|reply| Command::SetDeafened(deafened, reply))
            .await
    }

    pub async fn start_screen_share(
        &self,
        source_id: &str,
        preset: ScreenPreset,
    ) -> CoreResult<()> {
        let source_id = source_id.to_owned();
        self.request(|reply| Command::StartScreenShare(source_id, preset, reply))
            .await
    }

    pub async fn stop_screen_share(&self) -> CoreResult<()> {
        self.request(Command::StopScreenShare).await
    }

    pub async fn list_audio_devices(&self) -> CoreResult<AudioDevices> {
        self.deps
            .audio
            .list_devices()
            .await
            .map_err(CoreError::audio)
    }

    pub async fn select_audio_device(&self, kind: DeviceKind, id: &str) -> CoreResult<()> {
        self.deps
            .audio
            .select_device(kind, id)
            .await
            .map_err(CoreError::audio)
    }

    pub async fn list_screen_sources(&self) -> CoreResult<Vec<ScreenSourceInfo>> {
        self.deps
            .screen
            .list_sources()
            .await
            .map_err(CoreError::screen)
    }

    async fn request(&self, make: impl FnOnce(Reply) -> Command) -> CoreResult<()> {
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(make(tx))
            .await
            .map_err(|_| CoreError::Closed)?;
        rx.await.map_err(|_| CoreError::Closed)?
    }
}

struct Retry {
    attempt: u32,
    at: Instant,
}

struct Actor {
    deps: Dependencies,
    config: RoomConfig,
    events: broadcast::Sender<RoomEvent>,
    snapshot: Arc<Mutex<Snapshot>>,
    state: RoomState,
    /// Local participant first (when connected), then remotes in join order.
    participants: Vec<Participant>,
    muted: bool,
    deafened: bool,
    sharing: Option<ScreenPreset>,
    params: Option<JoinParams>,
    session_events: Option<mpsc::Receiver<TransportEvent>>,
    retry: Option<Retry>,
}

async fn next_event(rx: &mut Option<mpsc::Receiver<TransportEvent>>) -> Option<TransportEvent> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

impl Actor {
    async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        loop {
            tokio::select! {
                cmd = commands.recv() => match cmd {
                    Some(cmd) => self.handle(cmd).await,
                    None => {
                        self.teardown().await;
                        break;
                    }
                },
                event = next_event(&mut self.session_events) => match event {
                    Some(event) => self.on_transport_event(event).await,
                    None => self.on_connection_lost("transport closed".to_owned()).await,
                },
                () = sleep_until(self.retry.as_ref().map(|r| r.at)) => self.retry_connect().await,
            }
        }
    }

    async fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Join(params, reply) => {
                let _ = reply.send(self.join(params).await);
            }
            Command::Leave(reply) => {
                self.leave().await;
                let _ = reply.send(Ok(()));
            }
            Command::SetMuted(muted, reply) => {
                self.muted = muted;
                let _ = reply.send(self.apply_audio_state().await);
            }
            Command::SetDeafened(deafened, reply) => {
                self.deafened = deafened;
                let _ = reply.send(self.apply_audio_state().await);
            }
            Command::StartScreenShare(source, preset, reply) => {
                let _ = reply.send(self.start_screen_share(&source, preset).await);
            }
            Command::StopScreenShare(reply) => {
                self.stop_screen_share().await;
                let _ = reply.send(Ok(()));
            }
        }
    }

    fn is_active(&self) -> bool {
        matches!(
            self.state.status,
            RoomStatus::Connecting | RoomStatus::Connected | RoomStatus::Reconnecting
        )
    }

    fn effective_mic_muted(&self) -> bool {
        self.muted || self.deafened
    }

    async fn join(&mut self, params: JoinParams) -> CoreResult<()> {
        if self.is_active() {
            return Err(CoreError::AlreadyActive);
        }
        self.set_state(RoomState::new(RoomStatus::Connecting));

        if let Err(e) = self.establish(&params).await {
            self.reset_session().await;
            self.set_state(RoomState::error(e.to_string()));
            return Err(e);
        }
        if let Err(e) = self.deps.audio.start().await.map_err(CoreError::audio) {
            self.deps.transport.disconnect().await;
            self.reset_session().await;
            self.set_state(RoomState::error(e.to_string()));
            return Err(e);
        }
        self.params = Some(params);
        let audio_result = self.apply_audio_state().await;
        self.set_state(RoomState::new(RoomStatus::Connected));
        audio_result
    }

    /// Fetches a token and connects, replacing the participant list with the new session's.
    async fn establish(&mut self, params: &JoinParams) -> CoreResult<()> {
        let creds = self
            .deps
            .tokens
            .fetch_token(params)
            .await
            .map_err(CoreError::token)?;
        let session = self
            .deps
            .transport
            .connect(&creds.url, &creds.token)
            .await
            .map_err(CoreError::transport)?;

        let local = Participant {
            id: session.local_identity,
            name: params.name.clone(),
            muted: self.effective_mic_muted(),
            speaking: false,
            sharing_screen: self.sharing.is_some(),
            is_local: true,
        };
        self.participants = std::iter::once(local)
            .chain(session.participants.into_iter().map(Participant::from))
            .collect();
        self.session_events = Some(session.events);
        self.publish_participants();
        Ok(())
    }

    async fn apply_audio_state(&mut self) -> CoreResult<()> {
        let mic_muted = self.effective_mic_muted();
        if let Some(local) = self.participants.iter_mut().find(|p| p.is_local) {
            local.muted = mic_muted;
        }
        if self.params.is_none() {
            self.publish_snapshot();
            return Ok(());
        }
        self.publish_participants();
        self.deps
            .audio
            .set_playback_muted(self.deafened)
            .await
            .map_err(CoreError::audio)?;
        self.deps
            .audio
            .set_capture_muted(mic_muted)
            .await
            .map_err(CoreError::audio)?;
        if self.session_events.is_none() {
            return Ok(());
        }
        self.deps
            .transport
            .set_microphone_muted(mic_muted)
            .await
            .map_err(CoreError::transport)
    }

    async fn leave(&mut self) {
        if self.state.status == RoomStatus::Disconnected {
            return;
        }
        self.teardown().await;
        self.set_state(RoomState::new(RoomStatus::Disconnected));
    }

    async fn teardown(&mut self) {
        if self.params.is_some() {
            self.deps.transport.disconnect().await;
            self.deps.audio.stop().await;
        }
        self.reset_session().await;
    }

    async fn reset_session(&mut self) {
        if self.sharing.take().is_some() {
            self.deps.screen.stop().await;
        }
        self.params = None;
        self.session_events = None;
        self.retry = None;
        if !self.participants.is_empty() {
            self.participants.clear();
            self.publish_participants();
        }
    }

    async fn on_transport_event(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::ParticipantJoined(p) => {
                self.participants.retain(|existing| existing.id != p.id);
                self.participants.push(p.into());
            }
            TransportEvent::ParticipantLeft { id } => {
                self.participants.retain(|p| p.is_local || p.id != id);
            }
            TransportEvent::MuteChanged { id, muted } => {
                if let Some(p) = self.remote_mut(&id) {
                    p.muted = muted;
                }
            }
            TransportEvent::ScreenShareChanged { id, sharing } => {
                if let Some(p) = self.remote_mut(&id) {
                    p.sharing_screen = sharing;
                }
            }
            TransportEvent::ActiveSpeakers(ids) => {
                for p in &mut self.participants {
                    p.speaking = ids.contains(&p.id);
                }
            }
            TransportEvent::Disconnected { reason } => {
                self.on_connection_lost(reason).await;
                return;
            }
        }
        self.publish_participants();
    }

    fn remote_mut(&mut self, id: &str) -> Option<&mut Participant> {
        self.participants
            .iter_mut()
            .find(|p| !p.is_local && p.id == id)
    }

    async fn on_connection_lost(&mut self, reason: String) {
        self.session_events = None;
        if self.params.is_none() {
            return;
        }
        tracing::warn!(%reason, "connection lost, reconnecting");
        self.participants.retain(|p| p.is_local);
        for p in &mut self.participants {
            p.speaking = false;
        }
        self.publish_participants();
        self.set_state(RoomState::new(RoomStatus::Reconnecting));
        self.schedule_retry(0);
    }

    fn schedule_retry(&mut self, attempt: u32) {
        self.retry = Some(Retry {
            attempt,
            at: Instant::now() + self.config.backoff.delay(attempt),
        });
    }

    async fn retry_connect(&mut self) {
        let Some(Retry { attempt, .. }) = self.retry.take() else {
            return;
        };
        let Some(params) = self.params.clone() else {
            return;
        };
        match self.establish(&params).await {
            Ok(()) => {
                tracing::info!(attempt, "reconnected");
                let _ = self.apply_audio_state().await;
                if let Some(preset) = self.sharing {
                    if let Err(e) = self.deps.transport.publish_screen(preset).await {
                        tracing::warn!(error = %e, "failed to republish screen after reconnect");
                    }
                }
                self.set_state(RoomState::new(RoomStatus::Connected));
            }
            Err(e) if attempt + 1 >= self.config.backoff.max_attempts => {
                tracing::error!(error = %e, "giving up reconnecting");
                self.teardown().await;
                self.set_state(RoomState::error(format!(
                    "reconnect failed after {} attempts: {e}",
                    attempt + 1
                )));
            }
            Err(e) => {
                tracing::warn!(error = %e, attempt, "reconnect attempt failed");
                self.schedule_retry(attempt + 1);
            }
        }
    }

    async fn start_screen_share(&mut self, source: &str, preset: ScreenPreset) -> CoreResult<()> {
        if self.state.status != RoomStatus::Connected {
            return Err(CoreError::NotConnected);
        }
        if self.sharing.is_some() {
            self.stop_screen_share().await;
        }
        self.deps
            .screen
            .start(source, preset)
            .await
            .map_err(CoreError::screen)?;
        if let Err(e) = self.deps.transport.publish_screen(preset).await {
            self.deps.screen.stop().await;
            return Err(CoreError::transport(e));
        }
        self.sharing = Some(preset);
        self.set_local_sharing(true);
        Ok(())
    }

    async fn stop_screen_share(&mut self) {
        if self.sharing.take().is_none() {
            return;
        }
        if let Err(e) = self.deps.transport.unpublish_screen().await {
            tracing::warn!(error = %e, "failed to unpublish screen");
        }
        self.deps.screen.stop().await;
        self.set_local_sharing(false);
    }

    fn set_local_sharing(&mut self, sharing: bool) {
        if let Some(local) = self.participants.iter_mut().find(|p| p.is_local) {
            local.sharing_screen = sharing;
        }
        self.publish_participants();
    }

    fn set_state(&mut self, state: RoomState) {
        if self.state == state {
            return;
        }
        self.state = state.clone();
        self.publish_snapshot();
        let _ = self.events.send(RoomEvent::State(state));
    }

    fn publish_participants(&mut self) {
        self.publish_snapshot();
        let _ = self
            .events
            .send(RoomEvent::Participants(self.participants.clone()));
    }

    fn publish_snapshot(&self) {
        let mut snapshot = self.snapshot.lock().unwrap_or_else(PoisonError::into_inner);
        *snapshot = Snapshot {
            state: self.state.clone(),
            participants: self.participants.clone(),
            muted: self.muted,
            deafened: self.deafened,
            sharing: self.sharing,
        };
    }
}
