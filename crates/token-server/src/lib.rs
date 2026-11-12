//! Sheeshcord token server: issues LiveKit access tokens (see `CONTRACT.md`).

pub mod config;
pub mod token;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Extension;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub use config::{Config, ConfigError};
pub use token::{Claims, VideoGrant};

/// Wrong-password attempts allowed per client within [`FAILURE_WINDOW`].
pub const MAX_FAILURES: u32 = 10;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Deserialize)]
pub struct TokenRequest {
    pub room: String,
    pub name: String,
    /// Subscribe-only token for the WebView video viewer.
    #[serde(default)]
    pub viewer: bool,
    /// Shared group password (checked when the server has `ACCESS_PASSWORD`).
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenResponse {
    pub token: String,
    pub url: String,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}

struct AppState {
    config: Config,
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl AppState {
    fn is_blocked(&self, client: &str) -> bool {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        map.retain(|_, (_, since)| now.duration_since(*since) < FAILURE_WINDOW);
        map.get(client).is_some_and(|(n, _)| *n >= MAX_FAILURES)
    }

    fn record_failure(&self, client: &str) {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(client.to_owned())
            .or_insert((0, Instant::now()))
            .0 += 1;
    }
}

/// Compares in constant time with respect to the content of `a`/`b` (length may leak).
fn secure_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Client key for rate limiting. Behind the reverse proxy (only exposed through Caddy)
/// the last `X-Forwarded-For` entry is the address the proxy saw.
fn client_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .or_else(|| peer.map(|p| p.ip().to_string()))
        .unwrap_or_else(|| "unknown".to_owned())
}

pub fn router(config: Config) -> Router {
    let state = Arc::new(AppState {
        config,
        failures: Mutex::new(HashMap::new()),
    });
    Router::new()
        .route("/health", get(health))
        .route("/token", post(issue_token))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Serves the router on an already bound listener until `shutdown` resolves.
pub async fn serve(
    listener: TcpListener,
    config: Config,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        router(config).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn issue_token(
    State(state): State<Arc<AppState>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Result<Json<TokenRequest>, JsonRejection>,
) -> Response {
    let config = &state.config;
    let Json(req) = match body {
        Ok(body) => body,
        Err(rejection) => return error(StatusCode::BAD_REQUEST, rejection.body_text()),
    };
    if let Err(e) = token::validate_field("room", &req.room)
        .and_then(|()| token::validate_field("name", &req.name))
    {
        return error(StatusCode::BAD_REQUEST, e.to_string());
    }

    if let Some(expected) = &config.access_password {
        let client = client_key(&headers, peer.map(|Extension(ConnectInfo(addr))| addr));
        if state.is_blocked(&client) {
            tracing::warn!(%client, "too many wrong passwords");
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "too many wrong passwords, try again later",
            );
        }
        if !secure_eq(req.password.as_bytes(), expected.as_bytes()) {
            state.record_failure(&client);
            tracing::warn!(%client, "wrong group password");
            return error(StatusCode::UNAUTHORIZED, "wrong group password");
        }
    }

    let claims = token::build_claims_for(
        &config.api_key,
        &req.room,
        &req.name,
        config.token_ttl,
        token::unix_now(),
        req.viewer,
    );
    match token::encode(&claims, &config.api_secret) {
        Ok(token) => {
            tracing::info!(room = %claims.video.room, identity = %claims.sub, "issued token");
            Json(TokenResponse {
                token,
                url: config.livekit_url.clone(),
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to sign token");
            error(StatusCode::INTERNAL_SERVER_ERROR, "failed to sign token")
        }
    }
}
