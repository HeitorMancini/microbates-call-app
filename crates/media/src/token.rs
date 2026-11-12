//! [`TokenProvider`] that calls the Sheeshcord token-server (`POST {base}/token`).

use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use sheeshcord_core::{BoxError, Credentials, JoinParams, TokenProvider};

#[derive(Debug, Clone)]
pub struct HttpTokenProvider {
    client: reqwest::Client,
}

impl Default for HttpTokenProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct TokenRequest<'a> {
    room: &'a str,
    name: &'a str,
    viewer: bool,
    password: &'a str,
}

/// Maps what users type (`ws://host:7881`, `host:7881/`) to the token endpoint URL.
pub fn token_endpoint(server_url: &str) -> String {
    let url = server_url.trim().trim_end_matches('/');
    let url = if let Some(rest) = url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else if let Some(rest) = url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if url.starts_with("http://") || url.starts_with("https://") {
        url.to_owned()
    } else {
        format!("http://{url}")
    };
    format!("{url}/token")
}

impl HttpTokenProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Subscribe-only credentials for the WebView video viewer.
    pub async fn fetch_viewer_token(&self, params: &JoinParams) -> Result<Credentials, BoxError> {
        self.request(params, true).await
    }

    async fn request(&self, params: &JoinParams, viewer: bool) -> Result<Credentials, BoxError> {
        let endpoint = token_endpoint(&params.server_url);
        let res = self
            .client
            .post(&endpoint)
            .json(&TokenRequest {
                room: &params.room,
                name: &params.name,
                viewer,
                password: &params.password,
            })
            .send()
            .await
            .map_err(|e| format!("token-server inacessível em {endpoint}: {e}"))?;
        let status = res.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("senha do grupo incorreta".into());
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("muitas tentativas com senha errada; aguarde alguns minutos".into());
        }
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(format!("token-server respondeu {status}: {body}").into());
        }
        Ok(res.json::<Credentials>().await?)
    }
}

#[async_trait]
impl TokenProvider for HttpTokenProvider {
    async fn fetch_token(&self, params: &JoinParams) -> Result<Credentials, BoxError> {
        self.request(params, false).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_normalizes_schemes_and_slashes() {
        assert_eq!(
            token_endpoint(" ws://26.1.2.3:7881/ "),
            "http://26.1.2.3:7881/token"
        );
        assert_eq!(token_endpoint("wss://h"), "https://h/token");
        assert_eq!(token_endpoint("http://h:1"), "http://h:1/token");
        assert_eq!(token_endpoint("h:7881"), "http://h:7881/token");
    }
}
