use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde_json::{json, Value};
use token_server::{router, Claims, Config, TokenResponse, MAX_FAILURES};
use tower::ServiceExt;

const KEY: &str = "devkey";
const SECRET: &str = "devsecretdevsecretdevsecretdevsecret";
const URL: &str = "ws://26.1.2.3:7880";

fn config() -> Config {
    Config {
        api_key: KEY.to_owned(),
        api_secret: SECRET.to_owned(),
        livekit_url: URL.to_owned(),
        bind: "127.0.0.1:0".parse().unwrap(),
        token_ttl: Duration::from_secs(3600),
        access_password: None,
    }
}

fn protected() -> Router {
    router(Config {
        access_password: Some("segredo".to_owned()),
        ..config()
    })
}

async fn send_to(app: Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn send(req: Request<Body>) -> (StatusCode, Value) {
    send_to(router(config()), req).await
}

fn post_token(body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.into())
        .unwrap()
}

fn post_json(value: Value) -> Request<Body> {
    post_token(value.to_string())
}

fn decode(token: &str) -> Claims {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[KEY]);
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(SECRET.as_bytes()),
        &validation,
    )
    .unwrap()
    .claims
}

#[tokio::test]
async fn health_returns_ok() {
    let req = Request::get("/health").body(Body::empty()).unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn token_returns_url_and_valid_jwt() {
    let (status, body) = send(post_json(json!({ "room": "geral", "name": "heitor" }))).await;
    assert_eq!(status, StatusCode::OK);

    let res: TokenResponse = serde_json::from_value(body).unwrap();
    assert_eq!(res.url, URL);

    let claims = decode(&res.token);
    assert!(claims.video.room_join);
    assert_eq!(claims.video.room, "geral");
    assert_eq!(claims.name, "heitor");
    assert!(claims.sub.starts_with("heitor-"));
    assert!(claims.sub.len() > "heitor-".len());
    assert_eq!(claims.iss, KEY);

    let now = token_server::token::unix_now();
    assert!(claims.exp > now + 3500 && claims.exp <= now + 3600);
    assert!(claims.nbf <= now);
}

#[tokio::test]
async fn identities_differ_for_same_name() {
    let mut subs = Vec::new();
    for _ in 0..2 {
        let (_, body) = send(post_json(json!({ "room": "geral", "name": "ana" }))).await;
        let res: TokenResponse = serde_json::from_value(body).unwrap();
        subs.push(decode(&res.token).sub);
    }
    assert_ne!(subs[0], subs[1]);
}

#[tokio::test]
async fn token_signed_with_other_secret_is_rejected() {
    let (_, body) = send(post_json(json!({ "room": "r", "name": "n" }))).await;
    let res: TokenResponse = serde_json::from_value(body).unwrap();
    let result = jsonwebtoken::decode::<Claims>(
        &res.token,
        &DecodingKey::from_secret(b"wrong-secret"),
        &Validation::new(Algorithm::HS256),
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn invalid_inputs_return_400() {
    let long = "x".repeat(65);
    let cases = [
        json!({ "room": "", "name": "heitor" }),
        json!({ "room": "geral", "name": "" }),
        json!({ "room": "   ", "name": "heitor" }),
        json!({ "room": long, "name": "heitor" }),
        json!({ "room": "geral", "name": long }),
        json!({ "room": "geral" }),
        json!({ "room": 1, "name": "heitor" }),
    ];
    for case in cases {
        let (status, body) = send(post_json(case.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "case {case}");
        assert!(body["error"].is_string(), "case {case}: {body}");
    }
}

#[tokio::test]
async fn malformed_json_returns_400() {
    let (status, _) = send(post_token("{not json")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn exactly_64_chars_is_accepted() {
    let max = "y".repeat(64);
    let (status, _) = send(post_json(json!({ "room": max, "name": max }))).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn get_token_is_not_allowed() {
    let req = Request::get("/token").body(Body::empty()).unwrap();
    let (status, _) = send(req).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn password_is_required_when_configured() {
    let ok = json!({ "room": "geral", "name": "ana", "password": "segredo" });
    let (status, _) = send_to(protected(), post_json(ok)).await;
    assert_eq!(status, StatusCode::OK);

    let viewer = json!({ "room": "geral", "name": "ana", "password": "segredo", "viewer": true });
    let (status, _) = send_to(protected(), post_json(viewer)).await;
    assert_eq!(status, StatusCode::OK);

    for case in [
        json!({ "room": "geral", "name": "ana" }),
        json!({ "room": "geral", "name": "ana", "password": "errada" }),
        json!({ "room": "geral", "name": "ana", "password": "segredo2" }),
        json!({ "room": "geral", "name": "ana", "password": "" }),
    ] {
        let (status, body) = send_to(protected(), post_json(case.clone())).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "case {case}");
        assert!(body["error"].is_string());
    }
}

#[tokio::test]
async fn repeated_wrong_passwords_are_throttled_per_client() {
    let app = protected();
    let attempt = |password: &str, ip: &str| {
        Request::builder()
            .method(Method::POST)
            .uri("/token")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", ip)
            .body(Body::from(
                json!({ "room": "geral", "name": "ana", "password": password }).to_string(),
            ))
            .unwrap()
    };
    for _ in 0..MAX_FAILURES {
        let (status, _) = send_to(app.clone(), attempt("errada", "1.2.3.4")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _) = send_to(app.clone(), attempt("segredo", "1.2.3.4")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    let (status, _) = send_to(app, attempt("segredo", "5.6.7.8")).await;
    assert_eq!(status, StatusCode::OK);
}
