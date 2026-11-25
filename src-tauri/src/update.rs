//! Atualização OTA: baixa a versão nova em segundo plano e só instala quando o usuário pede.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const EVENT_UPDATE: &str = "update://state";
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum UpdateState {
    Idle,
    Downloading { percent: Option<u8> },
    Ready { version: String },
}

fn percent(downloaded: u64, total: Option<u64>) -> Option<u8> {
    let total = total.filter(|&t| t > 0)?;
    Some((downloaded.min(total) * 100 / total) as u8)
}

pub struct UpdateService {
    state: Mutex<UpdateState>,
    pending: Mutex<Option<(Update, Vec<u8>)>>,
}

impl UpdateService {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(UpdateState::Idle),
            pending: Mutex::new(None),
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set(&self, app: &AppHandle, next: UpdateState) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if *state == next {
                return;
            }
            *state = next.clone();
        }
        if let Err(err) = app.emit(EVENT_UPDATE, &next) {
            tracing::warn!(%err, "falha ao emitir {EVENT_UPDATE}");
        }
    }

    fn take_pending(&self) -> Option<(Update, Vec<u8>)> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

impl Default for UpdateService {
    fn default() -> Self {
        Self::new()
    }
}

async fn check_once(app: &AppHandle) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let svc = app.state::<UpdateService>();
    if matches!(svc.state(), UpdateState::Ready { .. }) {
        return Ok(());
    }
    let Some(update) = app.updater()?.check().await? else {
        return Ok(());
    };
    svc.set(app, UpdateState::Downloading { percent: None });
    let mut downloaded = 0u64;
    let progress_app = app.clone();
    let result = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                progress_app.state::<UpdateService>().set(
                    &progress_app,
                    UpdateState::Downloading {
                        percent: percent(downloaded, total),
                    },
                );
            },
            || {},
        )
        .await;
    match result {
        Ok(bytes) => {
            let version = update.version.clone();
            *svc.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some((update, bytes));
            svc.set(app, UpdateState::Ready { version });
            Ok(())
        }
        Err(e) => {
            svc.set(app, UpdateState::Idle);
            Err(e.into())
        }
    }
}

/// Verifica logo após abrir e depois periodicamente; falhas só vão para o log.
pub async fn run(app: AppHandle) {
    tokio::time::sleep(FIRST_CHECK_DELAY).await;
    loop {
        if let Err(err) = check_once(&app).await {
            tracing::warn!(%err, "verificação de atualização falhou");
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

#[tauri::command]
pub fn update_state(svc: tauri::State<'_, UpdateService>) -> UpdateState {
    svc.state()
}

/// Instala a atualização já baixada e reinicia o app.
#[tauri::command]
pub fn install_update(app: AppHandle, svc: tauri::State<'_, UpdateService>) -> Result<(), String> {
    let (update, bytes) = svc
        .take_pending()
        .ok_or_else(|| "nenhuma atualização pronta".to_owned())?;
    update.install(bytes).map_err(|e| e.to_string())?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn percent_is_clamped_and_needs_a_known_total() {
        assert_eq!(percent(50, Some(200)), Some(25));
        assert_eq!(percent(300, Some(200)), Some(100));
        assert_eq!(percent(10, None), None);
        assert_eq!(percent(10, Some(0)), None);
    }

    #[test]
    fn state_serializes_with_a_status_tag() {
        assert_eq!(
            serde_json::to_value(UpdateState::Ready {
                version: "1.2.3".into()
            })
            .unwrap(),
            json!({ "status": "ready", "version": "1.2.3" })
        );
        assert_eq!(
            serde_json::to_value(UpdateState::Downloading { percent: Some(4) }).unwrap(),
            json!({ "status": "downloading", "percent": 4 })
        );
        assert_eq!(
            serde_json::to_value(UpdateState::Idle).unwrap(),
            json!({ "status": "idle" })
        );
    }
}
