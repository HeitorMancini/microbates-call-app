//! App Tauri do Sheeshcord. Os comandos seguem o CONTRACT.md e delegam a um
//! [`room::RoomService`], que usa `sheeshcord_core` com a mídia real de `crates/media`
//! (ou fontes sintéticas com `SHEESHCORD_FAKE_MEDIA=1`).

pub mod model;
pub mod room;

use std::sync::Arc;

use sheeshcord_core::{AudioSource, Dependencies, ScreenSource as ScreenCapture};
use sheeshcord_media::{
    FakeAudio, FakeScreen, HttpTokenProvider, LiveKitTransport, MediaHub, TransportOptions,
};
use tauri::{AppHandle, Emitter, Manager, State};

use model::{
    AudioDevices, DeviceKind, Participant, RoomState, ScreenPreset, ScreenSource,
    ViewerCredentials, EVENT_PARTICIPANTS, EVENT_STATE,
};
use room::{EventSink, RoomError, RoomService};

/// Variável de ambiente que troca microfone/tela reais por tom de 440 Hz e barras de cor.
pub const FAKE_MEDIA_ENV: &str = "SHEESHCORD_FAKE_MEDIA";

struct TauriSink(AppHandle);

impl EventSink for TauriSink {
    fn state(&self, state: &RoomState) {
        if let Err(err) = self.0.emit(EVENT_STATE, state) {
            tracing::warn!(%err, "falha ao emitir {EVENT_STATE}");
        }
    }

    fn participants(&self, participants: &[Participant]) {
        if let Err(err) = self.0.emit(EVENT_PARTICIPANTS, participants) {
            tracing::warn!(%err, "falha ao emitir {EVENT_PARTICIPANTS}");
        }
    }
}

fn fake_media_enabled() -> bool {
    std::env::var(FAKE_MEDIA_ENV).is_ok_and(|v| v == "1")
}

/// Monta as dependências reais (LiveKit + token-server HTTP) com fontes de mídia reais
/// ou sintéticas.
pub fn media_dependencies(hub: &MediaHub, fake: bool) -> Dependencies {
    let (audio, screen): (Arc<dyn AudioSource>, Arc<dyn ScreenCapture>) = if fake {
        tracing::info!("{FAKE_MEDIA_ENV}=1: usando mídia sintética");
        (
            Arc::new(FakeAudio::new(hub.clone())),
            Arc::new(FakeScreen::new(hub.clone())),
        )
    } else {
        (
            Arc::new(sheeshcord_media::CpalAudio::new(hub.clone())),
            real_screen(hub),
        )
    };
    Dependencies {
        tokens: Arc::new(HttpTokenProvider::new()),
        transport: Arc::new(LiveKitTransport::with_options(
            hub.clone(),
            TransportOptions::default(),
        )),
        audio,
        screen,
    }
}

#[cfg(windows)]
fn real_screen(hub: &MediaHub) -> Arc<dyn ScreenCapture> {
    Arc::new(sheeshcord_media::WgcScreen::new(hub.clone()))
}

#[cfg(not(windows))]
fn real_screen(hub: &MediaHub) -> Arc<dyn ScreenCapture> {
    tracing::warn!("captura de tela só existe no Windows; usando padrão de teste");
    Arc::new(FakeScreen::new(hub.clone()))
}

#[tauri::command]
async fn join_room(
    svc: State<'_, RoomService>,
    server_url: String,
    room: String,
    name: String,
    password: Option<String>,
) -> Result<(), RoomError> {
    svc.join_room(
        &server_url,
        &room,
        &name,
        password.as_deref().unwrap_or_default(),
    )
    .await
}

#[tauri::command]
async fn leave_room(svc: State<'_, RoomService>) -> Result<(), RoomError> {
    svc.leave_room().await
}

#[tauri::command]
async fn set_muted(svc: State<'_, RoomService>, muted: bool) -> Result<(), RoomError> {
    svc.set_muted(muted).await
}

#[tauri::command]
async fn set_deafened(svc: State<'_, RoomService>, deafened: bool) -> Result<(), RoomError> {
    svc.set_deafened(deafened).await
}

#[tauri::command]
async fn list_audio_devices(svc: State<'_, RoomService>) -> Result<AudioDevices, RoomError> {
    svc.list_audio_devices().await
}

#[tauri::command]
async fn select_audio_device(
    svc: State<'_, RoomService>,
    kind: DeviceKind,
    id: String,
) -> Result<(), RoomError> {
    svc.select_audio_device(kind, &id).await
}

#[tauri::command]
async fn list_screen_sources(svc: State<'_, RoomService>) -> Result<Vec<ScreenSource>, RoomError> {
    svc.list_screen_sources().await
}

#[tauri::command]
async fn start_screen_share(
    svc: State<'_, RoomService>,
    source_id: String,
    preset: ScreenPreset,
) -> Result<(), RoomError> {
    svc.start_screen_share(&source_id, preset).await
}

#[tauri::command]
async fn stop_screen_share(svc: State<'_, RoomService>) -> Result<(), RoomError> {
    svc.stop_screen_share().await
}

#[tauri::command]
async fn viewer_credentials(
    svc: State<'_, RoomService>,
    tokens: State<'_, HttpTokenProvider>,
) -> Result<ViewerCredentials, RoomError> {
    let joined = svc.joined()?;
    let creds = tokens
        .fetch_viewer_token(&joined)
        .await
        .map_err(|e| RoomError::Token(e.to_string()))?;
    Ok(ViewerCredentials {
        url: creds.url,
        token: creds.token,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let sink = Arc::new(TauriSink(app.handle().clone()));
            let hub = MediaHub::default();
            let deps = media_dependencies(&hub, fake_media_enabled());
            // O controlador da sala cria tarefas Tokio: precisa do runtime do Tauri.
            let service =
                tauri::async_runtime::block_on(async move { RoomService::new(deps, sink) });
            app.manage(service);
            app.manage(HttpTokenProvider::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            join_room,
            leave_room,
            set_muted,
            set_deafened,
            list_audio_devices,
            select_audio_device,
            list_screen_sources,
            start_screen_share,
            stop_screen_share,
            viewer_credentials,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Sheeshcord");
}
