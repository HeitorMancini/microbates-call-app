//! Serviço de sala: valida a entrada dos comandos, delega para o
//! [`RoomController`] de `sheeshcord_core` e repassa os eventos (`room://state`,
//! `room://participants`) a um [`EventSink`]. O vídeo é renderizado pela própria WebView.

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use sheeshcord_core::{
    CoreError, Dependencies, JoinParams, RoomConfig, RoomController, RoomEvent, RoomStatus,
};
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::model::{AudioDevices, DeviceKind, Participant, RoomState, ScreenPreset, ScreenSource};

pub const MAX_FIELD_LEN: usize = 64;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RoomError {
    #[error("Informe o endereço do servidor.")]
    EmptyServerUrl,
    #[error("Endereço do servidor inválido: use ws://, wss://, http:// ou https://.")]
    InvalidServerUrl,
    #[error("O campo {0} é obrigatório.")]
    EmptyField(&'static str),
    #[error("O campo {0} aceita no máximo 64 caracteres.")]
    FieldTooLong(&'static str),
    #[error("Você já está em uma sala.")]
    AlreadyConnected,
    #[error("Você não está em nenhuma sala.")]
    NotConnected,
    #[error("Dispositivo de áudio não encontrado: {0}")]
    UnknownDevice(String),
    #[error("Fonte de tela não encontrada: {0}")]
    UnknownScreenSource(String),
    #[error("Não foi possível obter acesso à sala: {0}")]
    Token(String),
    #[error("Falha na conexão com a sala: {0}")]
    Transport(String),
    #[error("Erro de áudio: {0}")]
    Audio(String),
    #[error("Erro ao compartilhar a tela: {0}")]
    Screen(String),
    #[error("O serviço da sala foi encerrado.")]
    Closed,
}

impl From<CoreError> for RoomError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::NotConnected => Self::NotConnected,
            CoreError::AlreadyActive => Self::AlreadyConnected,
            CoreError::Token(m) => Self::Token(m),
            CoreError::Transport(m) => Self::Transport(m),
            CoreError::Audio(m) => Self::Audio(m),
            CoreError::Screen(m) => Self::Screen(m),
            CoreError::Closed => Self::Closed,
        }
    }
}

impl Serialize for RoomError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Destino dos eventos Rust -> frontend. Em produção é o `AppHandle`.
pub trait EventSink: Send + Sync {
    fn state(&self, state: &RoomState);
    fn participants(&self, participants: &[Participant]);
}

fn validate_field(label: &'static str, value: &str) -> Result<String, RoomError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RoomError::EmptyField(label));
    }
    if trimmed.chars().count() > MAX_FIELD_LEN {
        return Err(RoomError::FieldTooLong(label));
    }
    Ok(trimmed.to_owned())
}

fn validate_server_url(value: &str) -> Result<String, RoomError> {
    let url = value.trim();
    if url.is_empty() {
        return Err(RoomError::EmptyServerUrl);
    }
    let rest = ["ws://", "wss://", "http://", "https://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
        .ok_or(RoomError::InvalidServerUrl)?;
    if rest.is_empty() || rest.starts_with('/') || rest.contains(char::is_whitespace) {
        return Err(RoomError::InvalidServerUrl);
    }
    Ok(url.to_owned())
}

#[derive(Default)]
struct SelectedDevices {
    input: Option<String>,
    output: Option<String>,
}

pub struct RoomService {
    controller: RoomController,
    selected: Mutex<SelectedDevices>,
    joined: Mutex<Option<JoinParams>>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for RoomService {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl RoomService {
    /// Cria o serviço; precisa ser chamado dentro de um runtime Tokio.
    pub fn new(deps: Dependencies, sink: Arc<dyn EventSink>) -> Self {
        let controller = RoomController::new(deps, RoomConfig::default());
        let forward = tokio::spawn(forward_room_events(
            controller.clone(),
            controller.subscribe(),
            Arc::clone(&sink),
        ));
        Self {
            controller,
            selected: Mutex::default(),
            joined: Mutex::default(),
            tasks: vec![forward],
        }
    }

    /// Dados da sala atual (servidor, sala, nome), usados para abrir o visualizador de vídeo.
    pub fn joined(&self) -> Result<JoinParams, RoomError> {
        self.ensure_connected()?;
        self.joined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(RoomError::NotConnected)
    }

    fn ensure_connected(&self) -> Result<(), RoomError> {
        if self.controller.snapshot().state.status == RoomStatus::Connected {
            Ok(())
        } else {
            Err(RoomError::NotConnected)
        }
    }

    pub async fn join_room(
        &self,
        server_url: &str,
        room: &str,
        name: &str,
        password: &str,
    ) -> Result<(), RoomError> {
        let params = JoinParams {
            server_url: validate_server_url(server_url)?,
            room: validate_field("sala", room)?,
            name: validate_field("nome", name)?,
            password: password.to_owned(),
        };
        tracing::info!(server_url = %params.server_url, room = %params.room, "entrando na sala");
        self.controller.join(params.clone()).await?;
        *self.joined.lock().unwrap_or_else(PoisonError::into_inner) = Some(params);
        Ok(())
    }

    pub async fn leave_room(&self) -> Result<(), RoomError> {
        self.joined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        Ok(self.controller.leave().await?)
    }

    pub async fn set_muted(&self, muted: bool) -> Result<(), RoomError> {
        self.ensure_connected()?;
        // Ensurdecido implica mudo; desmutar também tira o ensurdecimento.
        if !muted && self.controller.snapshot().deafened {
            self.controller.set_deafened(false).await?;
        }
        Ok(self.controller.set_muted(muted).await?)
    }

    pub async fn set_deafened(&self, deafened: bool) -> Result<(), RoomError> {
        self.ensure_connected()?;
        Ok(self.controller.set_deafened(deafened).await?)
    }

    pub fn is_deafened(&self) -> bool {
        self.controller.snapshot().deafened
    }

    pub async fn list_audio_devices(&self) -> Result<AudioDevices, RoomError> {
        Ok(self.controller.list_audio_devices().await?)
    }

    pub async fn select_audio_device(&self, kind: DeviceKind, id: &str) -> Result<(), RoomError> {
        let devices = self.list_audio_devices().await?;
        let list = match kind {
            DeviceKind::Input => &devices.inputs,
            DeviceKind::Output => &devices.outputs,
        };
        if !list.iter().any(|d| d.id == id) {
            return Err(RoomError::UnknownDevice(id.to_owned()));
        }
        self.controller.select_audio_device(kind, id).await?;
        let mut selected = self.selected.lock().unwrap_or_else(PoisonError::into_inner);
        match kind {
            DeviceKind::Input => selected.input = Some(id.to_owned()),
            DeviceKind::Output => selected.output = Some(id.to_owned()),
        }
        Ok(())
    }

    pub fn selected_device(&self, kind: DeviceKind) -> Option<String> {
        let selected = self.selected.lock().unwrap_or_else(PoisonError::into_inner);
        match kind {
            DeviceKind::Input => selected.input.clone(),
            DeviceKind::Output => selected.output.clone(),
        }
    }

    pub async fn list_screen_sources(&self) -> Result<Vec<ScreenSource>, RoomError> {
        Ok(self.controller.list_screen_sources().await?)
    }

    pub async fn start_screen_share(
        &self,
        source_id: &str,
        preset: ScreenPreset,
    ) -> Result<(), RoomError> {
        self.ensure_connected()?;
        let sources = self.list_screen_sources().await?;
        if !sources.iter().any(|s| s.id == source_id) {
            return Err(RoomError::UnknownScreenSource(source_id.to_owned()));
        }
        tracing::info!(source_id, ?preset, "iniciando compartilhamento de tela");
        Ok(self
            .controller
            .start_screen_share(source_id, preset)
            .await?)
    }

    pub async fn stop_screen_share(&self) -> Result<(), RoomError> {
        self.ensure_connected()?;
        Ok(self.controller.stop_screen_share().await?)
    }

    pub fn active_share(&self) -> Option<ScreenPreset> {
        self.controller.snapshot().sharing
    }
}

async fn forward_room_events(
    controller: RoomController,
    mut events: tokio::sync::broadcast::Receiver<RoomEvent>,
    sink: Arc<dyn EventSink>,
) {
    loop {
        match events.recv().await {
            Ok(RoomEvent::State(state)) => sink.state(&state),
            Ok(RoomEvent::Participants(list)) => sink.participants(&list),
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "eventos da sala descartados; reenviando o estado atual"
                );
                let snapshot = controller.snapshot();
                sink.state(&snapshot.state);
                sink.participants(&snapshot.participants);
            }
            Err(RecvError::Closed) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sheeshcord_core::mock::{MockAudio, MockScreen, MockTokenProvider, MockTransport};

    use super::*;
    use crate::model::ConnectionStatus;

    #[derive(Debug, Clone, PartialEq)]
    enum Event {
        State(RoomState),
        Participants(Vec<Participant>),
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);

    impl EventSink for Recorder {
        fn state(&self, state: &RoomState) {
            self.0.lock().unwrap().push(Event::State(state.clone()));
        }
        fn participants(&self, participants: &[Participant]) {
            self.0
                .lock()
                .unwrap()
                .push(Event::Participants(participants.to_vec()));
        }
    }

    impl Recorder {
        /// Espera os eventos assíncronos chegarem e devolve todos os acumulados.
        async fn take_when(&self, done: impl Fn(&[Event]) -> bool) -> Vec<Event> {
            for _ in 0..200 {
                if done(&self.0.lock().unwrap()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            std::mem::take(&mut *self.0.lock().unwrap())
        }

        async fn last_local(&self) -> Participant {
            tokio::time::sleep(Duration::from_millis(30)).await;
            self.0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find_map(|e| match e {
                    Event::Participants(p) => p.first().cloned(),
                    _ => None,
                })
                .expect("nenhum participante emitido")
        }
    }

    struct Fixture {
        svc: RoomService,
        rec: Arc<Recorder>,
        tokens: Arc<MockTokenProvider>,
        screen: Arc<MockScreen>,
    }

    fn service() -> Fixture {
        let rec = Arc::new(Recorder::default());
        let tokens = Arc::new(MockTokenProvider::default());
        let screen = Arc::new(MockScreen::default());
        let deps = Dependencies {
            tokens: tokens.clone(),
            transport: Arc::new(MockTransport::default()),
            audio: Arc::new(MockAudio::default()),
            screen: screen.clone(),
        };
        Fixture {
            svc: RoomService::new(deps, rec.clone()),
            rec,
            tokens,
            screen,
        }
    }

    fn connected(events: &[Event]) -> bool {
        events
            .iter()
            .any(|e| *e == Event::State(RoomState::new(ConnectionStatus::Connected)))
    }

    async fn joined() -> Fixture {
        let f = service();
        f.svc
            .join_room("ws://26.1.2.3:7880", "galera", "Ana", "")
            .await
            .unwrap();
        f.rec.take_when(connected).await;
        f
    }

    #[tokio::test]
    async fn join_emits_connecting_connected_and_local_participant() {
        let Fixture { svc, rec, .. } = service();
        svc.join_room(" ws://26.1.2.3:7880 ", " galera ", " Ana ", "")
            .await
            .unwrap();
        let events = rec.take_when(connected).await;
        assert_eq!(
            events[0],
            Event::State(RoomState::new(ConnectionStatus::Connecting))
        );
        assert!(connected(&events));
        let participants = events
            .iter()
            .find_map(|e| match e {
                Event::Participants(p) => Some(p),
                _ => None,
            })
            .expect("esperava participantes");
        assert_eq!(participants.len(), 1);
        assert_eq!(participants[0].name, "Ana");
        let p = &participants[0];
        assert!(p.is_local && !p.muted && !p.sharing_screen);
    }

    #[tokio::test]
    async fn join_validates_input() {
        let Fixture { svc, rec, .. } = service();
        assert_eq!(
            svc.join_room("", "s", "n", "").await,
            Err(RoomError::EmptyServerUrl)
        );
        assert_eq!(
            svc.join_room("ftp://x", "s", "n", "").await,
            Err(RoomError::InvalidServerUrl)
        );
        assert_eq!(
            svc.join_room("ws://", "s", "n", "").await,
            Err(RoomError::InvalidServerUrl)
        );
        assert_eq!(
            svc.join_room("ws://h", "  ", "n", "").await,
            Err(RoomError::EmptyField("sala"))
        );
        let long = "x".repeat(65);
        assert_eq!(
            svc.join_room("ws://h", "s", &long, "").await,
            Err(RoomError::FieldTooLong("nome"))
        );
        assert!(rec.take_when(|_| false).await.is_empty(), "nada emitido");
        assert!(svc
            .join_room("ws://h", "s", &"x".repeat(64), "")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn join_twice_is_rejected() {
        let f = joined().await;
        assert_eq!(
            f.svc.join_room("ws://h", "s", "n", "").await,
            Err(RoomError::AlreadyConnected)
        );
    }

    #[tokio::test]
    async fn token_failure_is_reported_in_state_and_result() {
        let f = service();
        f.tokens.set_error(Some("sala lotada"));
        let err = f.svc.join_room("ws://h", "s", "n", "").await.unwrap_err();
        assert!(matches!(&err, RoomError::Token(m) if m.contains("sala lotada")));
        let events = f
            .rec
            .take_when(|e| {
                e.iter()
                    .any(|e| matches!(e, Event::State(s) if s.error.is_some()))
            })
            .await;
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::State(s) if s.status == ConnectionStatus::Error)));
        f.tokens.set_error(None);
        assert!(f.svc.join_room("ws://h", "s", "n", "").await.is_ok());
    }

    #[tokio::test]
    async fn leave_emits_disconnected_and_empty_list_and_allows_rejoin() {
        let f = joined().await;
        f.svc.leave_room().await.unwrap();
        let disconnected = Event::State(RoomState::new(ConnectionStatus::Disconnected));
        let events = f.rec.take_when(|e| e.contains(&disconnected)).await;
        assert!(events.contains(&disconnected));
        assert!(events.contains(&Event::Participants(vec![])));
        assert_eq!(f.svc.set_muted(true).await, Err(RoomError::NotConnected));
        assert!(f.svc.join_room("ws://h", "s", "n", "").await.is_ok());
    }

    #[tokio::test]
    async fn mute_requires_connection_and_updates_participant() {
        let f = service();
        assert_eq!(f.svc.set_muted(true).await, Err(RoomError::NotConnected));

        let f = joined().await;
        f.svc.set_muted(true).await.unwrap();
        assert!(f.rec.last_local().await.muted);
        f.svc.set_muted(false).await.unwrap();
        assert!(!f.rec.last_local().await.muted);
    }

    #[tokio::test]
    async fn deafen_mutes_and_restores_previous_mute_state() {
        let Fixture { svc, rec, .. } = joined().await;
        svc.set_deafened(true).await.unwrap();
        assert!(svc.is_deafened());
        assert!(rec.last_local().await.muted);
        svc.set_deafened(false).await.unwrap();
        assert!(
            !rec.last_local().await.muted,
            "volta a falar se não estava mudo"
        );

        svc.set_muted(true).await.unwrap();
        svc.set_deafened(true).await.unwrap();
        svc.set_deafened(false).await.unwrap();
        assert!(rec.last_local().await.muted, "continua mudo se já estava");
    }

    #[tokio::test]
    async fn unmuting_while_deafened_clears_deafen() {
        let Fixture { svc, rec, .. } = joined().await;
        svc.set_deafened(true).await.unwrap();
        svc.set_muted(false).await.unwrap();
        assert!(!svc.is_deafened());
        assert!(!rec.last_local().await.muted);
    }

    #[tokio::test]
    async fn audio_devices_can_be_listed_and_selected() {
        let Fixture { svc, .. } = service();
        let devices = svc.list_audio_devices().await.unwrap();
        let (input, output) = (devices.inputs[0].id.clone(), devices.outputs[0].id.clone());
        svc.select_audio_device(DeviceKind::Input, &input)
            .await
            .unwrap();
        svc.select_audio_device(DeviceKind::Output, &output)
            .await
            .unwrap();
        assert_eq!(svc.selected_device(DeviceKind::Input), Some(input.clone()));
        assert_eq!(svc.selected_device(DeviceKind::Output), Some(output));
        assert_eq!(
            svc.select_audio_device(DeviceKind::Output, &input).await,
            Err(RoomError::UnknownDevice(input))
        );
    }

    #[tokio::test]
    async fn device_selection_survives_leave() {
        let Fixture { svc, .. } = joined().await;
        let input = svc.list_audio_devices().await.unwrap().inputs[0].id.clone();
        svc.select_audio_device(DeviceKind::Input, &input)
            .await
            .unwrap();
        svc.leave_room().await.unwrap();
        assert_eq!(svc.selected_device(DeviceKind::Input), Some(input));
    }

    #[tokio::test]
    async fn screen_share_start_and_stop() {
        let Fixture {
            svc, rec, screen, ..
        } = joined().await;
        let source = svc.list_screen_sources().await.unwrap()[0].id.clone();
        svc.start_screen_share(&source, ScreenPreset::P1080Fps60)
            .await
            .unwrap();
        assert!(rec.last_local().await.sharing_screen);
        assert_eq!(svc.active_share(), Some(ScreenPreset::P1080Fps60));
        assert_eq!(screen.active(), Some((source, ScreenPreset::P1080Fps60)));
        svc.stop_screen_share().await.unwrap();
        assert!(!rec.last_local().await.sharing_screen);
        assert_eq!(svc.active_share(), None);
        assert_eq!(screen.active(), None);
    }

    #[tokio::test]
    async fn screen_share_rejects_unknown_source_and_disconnected() {
        let f = service();
        let source = f.svc.list_screen_sources().await.unwrap()[0].id.clone();
        assert_eq!(
            f.svc
                .start_screen_share(&source, ScreenPreset::P720Fps30)
                .await,
            Err(RoomError::NotConnected)
        );
        let f = joined().await;
        assert_eq!(
            f.svc
                .start_screen_share("nope", ScreenPreset::P720Fps30)
                .await,
            Err(RoomError::UnknownScreenSource("nope".into()))
        );
    }

    #[test]
    fn errors_serialize_as_messages() {
        let v = serde_json::to_value(RoomError::NotConnected).unwrap();
        assert_eq!(v, serde_json::json!("Você não está em nenhuma sala."));
        assert_eq!(
            RoomError::from(CoreError::AlreadyActive),
            RoomError::AlreadyConnected
        );
    }
}
