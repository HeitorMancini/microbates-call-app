use std::net::SocketAddr;
use std::time::Duration;

use thiserror::Error;

pub const DEFAULT_BIND: &str = "0.0.0.0:7881";
pub const DEFAULT_LIVEKIT_URL: &str = "ws://127.0.0.1:7880";
pub const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid value for {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
}

#[derive(Clone)]
pub struct Config {
    pub api_key: String,
    pub api_secret: String,
    pub livekit_url: String,
    pub bind: SocketAddr,
    pub token_ttl: Duration,
    /// Shared group password required on `/token`. `None` disables the check (local dev only).
    pub access_password: Option<String>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("api_key", &self.api_key)
            .field("api_secret", &"<redacted>")
            .field("livekit_url", &self.livekit_url)
            .field("bind", &self.bind)
            .field("token_ttl", &self.token_ttl)
            .field(
                "access_password",
                &self.access_password.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Builds the config from an arbitrary variable source (testable without touching the process env).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |name: &'static str| lookup(name).filter(|v| !v.trim().is_empty());

        let api_key = get("LIVEKIT_API_KEY").ok_or(ConfigError::Missing("LIVEKIT_API_KEY"))?;
        let api_secret =
            get("LIVEKIT_API_SECRET").ok_or(ConfigError::Missing("LIVEKIT_API_SECRET"))?;
        let livekit_url = get("LIVEKIT_URL").unwrap_or_else(|| DEFAULT_LIVEKIT_URL.to_owned());
        if !(livekit_url.starts_with("ws://") || livekit_url.starts_with("wss://")) {
            return Err(ConfigError::Invalid {
                name: "LIVEKIT_URL",
                reason: "must start with ws:// or wss://".to_owned(),
            });
        }
        let bind = get("TOKEN_SERVER_BIND")
            .unwrap_or_else(|| DEFAULT_BIND.to_owned())
            .parse()
            .map_err(|e: std::net::AddrParseError| ConfigError::Invalid {
                name: "TOKEN_SERVER_BIND",
                reason: e.to_string(),
            })?;

        Ok(Self {
            api_key,
            api_secret,
            livekit_url,
            bind,
            token_ttl: DEFAULT_TOKEN_TTL,
            access_password: get("ACCESS_PASSWORD"),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn uses_defaults_for_optional_vars() {
        let cfg = Config::from_lookup(lookup(&[
            ("LIVEKIT_API_KEY", "k"),
            ("LIVEKIT_API_SECRET", "s"),
        ]))
        .unwrap();
        assert_eq!(cfg.livekit_url, DEFAULT_LIVEKIT_URL);
        assert_eq!(cfg.bind, DEFAULT_BIND.parse().unwrap());
    }

    #[test]
    fn reads_all_vars() {
        let cfg = Config::from_lookup(lookup(&[
            ("LIVEKIT_API_KEY", "k"),
            ("LIVEKIT_API_SECRET", "s"),
            ("LIVEKIT_URL", "ws://26.1.2.3:7880"),
            ("TOKEN_SERVER_BIND", "127.0.0.1:9000"),
        ]))
        .unwrap();
        assert_eq!(cfg.livekit_url, "ws://26.1.2.3:7880");
        assert_eq!(cfg.bind, "127.0.0.1:9000".parse().unwrap());
    }

    #[test]
    fn missing_secret_is_an_error() {
        let err = Config::from_lookup(lookup(&[("LIVEKIT_API_KEY", "k")])).unwrap_err();
        assert_eq!(err, ConfigError::Missing("LIVEKIT_API_SECRET"));
    }

    #[test]
    fn invalid_bind_is_an_error() {
        let err = Config::from_lookup(lookup(&[
            ("LIVEKIT_API_KEY", "k"),
            ("LIVEKIT_API_SECRET", "s"),
            ("TOKEN_SERVER_BIND", "not-an-addr"),
        ]))
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid {
                name: "TOKEN_SERVER_BIND",
                ..
            }
        ));
    }

    #[test]
    fn invalid_livekit_url_is_an_error() {
        let err = Config::from_lookup(lookup(&[
            ("LIVEKIT_API_KEY", "k"),
            ("LIVEKIT_API_SECRET", "s"),
            ("LIVEKIT_URL", "http://x:7880"),
        ]))
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid {
                name: "LIVEKIT_URL",
                ..
            }
        ));
    }

    #[test]
    fn debug_redacts_secret() {
        let cfg = Config::from_lookup(lookup(&[
            ("LIVEKIT_API_KEY", "k"),
            ("LIVEKIT_API_SECRET", "supersecret"),
        ]))
        .unwrap();
        assert!(!format!("{cfg:?}").contains("supersecret"));
    }

    #[test]
    fn reads_access_password_and_redacts_it() {
        let cfg = Config::from_lookup(lookup(&[
            ("LIVEKIT_API_KEY", "k"),
            ("LIVEKIT_API_SECRET", "s"),
            ("ACCESS_PASSWORD", "senha-do-grupo"),
        ]))
        .unwrap();
        assert_eq!(cfg.access_password.as_deref(), Some("senha-do-grupo"));
        assert!(!format!("{cfg:?}").contains("senha-do-grupo"));
    }
}
