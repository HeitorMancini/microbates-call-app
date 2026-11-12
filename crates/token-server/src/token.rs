use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_FIELD_LEN: usize = 64;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("`{0}` must not be empty")]
    Empty(&'static str),
    #[error("`{0}` must be at most {MAX_FIELD_LEN} characters")]
    TooLong(&'static str),
}

/// LiveKit `video` grant (subset used by Sheeshcord).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoGrant {
    pub room_join: bool,
    pub room: String,
    pub can_publish: bool,
    pub can_subscribe: bool,
    pub can_publish_data: bool,
}

/// Claims of a LiveKit access token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    pub name: String,
    pub nbf: u64,
    pub exp: u64,
    pub jti: String,
    pub video: VideoGrant,
}

pub fn validate_field(field: &'static str, value: &str) -> Result<(), ValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ValidationError::Empty(field));
    }
    if value.chars().count() > MAX_FIELD_LEN {
        return Err(ValidationError::TooLong(field));
    }
    Ok(())
}

/// Identity is `name` plus a random suffix so two people using the same nickname never collide.
pub fn unique_identity(name: &str) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    format!("{}-{}", name.trim(), &suffix[..8])
}

/// Identities starting with this prefix belong to the subscribe-only video viewer that the
/// WebView opens next to the native participant; the native client hides them from the roster.
pub const VIEWER_IDENTITY_PREFIX: &str = "viewer~";

pub fn build_claims(api_key: &str, room: &str, name: &str, ttl: Duration, now: u64) -> Claims {
    build_claims_for(api_key, room, name, ttl, now, false)
}

pub fn build_claims_for(
    api_key: &str,
    room: &str,
    name: &str,
    ttl: Duration,
    now: u64,
    viewer: bool,
) -> Claims {
    let identity = if viewer {
        format!("{VIEWER_IDENTITY_PREFIX}{}", unique_identity(name))
    } else {
        unique_identity(name)
    };
    Claims {
        iss: api_key.to_owned(),
        jti: identity.clone(),
        sub: identity,
        name: name.trim().to_owned(),
        nbf: now,
        exp: now + ttl.as_secs(),
        video: VideoGrant {
            room_join: true,
            room: room.trim().to_owned(),
            can_publish: !viewer,
            can_subscribe: true,
            can_publish_data: !viewer,
        },
    }
}

pub fn encode(claims: &Claims, api_secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(api_secret.as_bytes()),
    )
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{DecodingKey, Validation};

    use super::*;

    #[test]
    fn rejects_empty_and_whitespace() {
        assert_eq!(
            validate_field("room", ""),
            Err(ValidationError::Empty("room"))
        );
        assert_eq!(
            validate_field("room", "   "),
            Err(ValidationError::Empty("room"))
        );
    }

    #[test]
    fn length_limit_counts_chars_not_bytes() {
        let ok = "é".repeat(MAX_FIELD_LEN);
        assert!(validate_field("name", &ok).is_ok());
        let too_long = "a".repeat(MAX_FIELD_LEN + 1);
        assert_eq!(
            validate_field("name", &too_long),
            Err(ValidationError::TooLong("name"))
        );
    }

    #[test]
    fn identities_are_unique_and_prefixed_with_name() {
        let a = unique_identity("heitor");
        let b = unique_identity("heitor");
        assert_ne!(a, b);
        assert!(a.starts_with("heitor-"));
    }

    #[test]
    fn viewer_tokens_are_subscribe_only_with_prefixed_identity() {
        let c = build_claims_for("key", "sala", "ana", Duration::from_secs(60), 0, true);
        assert!(c.sub.starts_with(VIEWER_IDENTITY_PREFIX));
        assert!(c.video.can_subscribe && !c.video.can_publish && !c.video.can_publish_data);
        let c = build_claims("key", "sala", "ana", Duration::from_secs(60), 0);
        assert!(!c.sub.starts_with(VIEWER_IDENTITY_PREFIX) && c.video.can_publish);
    }

    #[test]
    fn encoded_token_roundtrips_with_hs256() {
        let claims = build_claims("key", "sala", "ana", Duration::from_secs(60), unix_now());
        let token = encode(&claims, "secret").unwrap();

        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&["key"]);
        let decoded = jsonwebtoken::decode::<Claims>(
            &token,
            &DecodingKey::from_secret(b"secret"),
            &validation,
        )
        .unwrap();
        assert_eq!(decoded.claims, claims);
        assert_eq!(decoded.header.alg, Algorithm::HS256);
    }
}
