//! Sheeshcord token server: issues LiveKit access tokens (see `CONTRACT.md`).

pub mod config;
pub mod token;

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub use config::{Config, ConfigError};
pub use token::{Claims, VideoGrant};

#[derive(Debug, Clone, Deserialize)]
pub struct TokenRequest {
    pub room: String,
    pub name: String,
    /// Subscribe-only token for the WebView video viewer.
    #[serde(default)]
    pub viewer: bool,
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

pub fn router(config: Config) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/token", post(issue_token))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(config))
}

/// Serves the router on an already bound listener until `shutdown` resolves.
pub async fn serve(
    listener: TcpListener,
    config: Config,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router(config))
        .with_graceful_shutdown(shutdown)
        .await
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn issue_token(
    State(config): State<Arc<Config>>,
    body: Result<Json<TokenRequest>, JsonRejection>,
) -> Response {
    let Json(req) = match body {
        Ok(body) => body,
        Err(rejection) => return error(StatusCode::BAD_REQUEST, rejection.body_text()),
    };
    if let Err(e) = token::validate_field("room", &req.room)
        .and_then(|()| token::validate_field("name", &req.name))
    {
        return error(StatusCode::BAD_REQUEST, e.to_string());
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
