use std::path::PathBuf;
use std::time::Duration;

use futures_util::StreamExt;
use integration_tests::{find_livekit_server, LiveKitServer, TokenServer, API_KEY, API_SECRET};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use livekit_api::services::room::{CreateRoomOptions, RoomClient};
use livekit_protocol::{signal_response, SignalResponse};
use prost::Message as _;
use serde_json::{json, Value};
use token_server::{Claims, TokenResponse};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

macro_rules! require_livekit {
    () => {
        match find_livekit_server() {
            Some(bin) => bin,
            None => {
                eprintln!(
                    "SKIPPED: livekit-server not found in PATH, LIVEKIT_SERVER_BIN or %USERPROFILE%\\.local\\bin"
                );
                return;
            }
        }
    };
}

fn log_path(test: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("livekit-{test}.log"))
}

async fn request_token(token_server: &TokenServer, room: &str, name: &str) -> TokenResponse {
    let res = reqwest::Client::new()
        .post(format!("{}/token", token_server.url()))
        .json(&json!({ "room": room, "name": name }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    res.json().await.unwrap()
}

fn identity_of(token: &str) -> String {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[API_KEY]);
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(API_SECRET.as_bytes()),
        &validation,
    )
    .unwrap()
    .claims
    .sub
}

fn rtc_url(base: &str, token: &str) -> String {
    format!("{base}/rtc?access_token={token}&auto_subscribe=1&sdk=rust&version=0.1.0&protocol=15")
}

/// Opens the LiveKit signaling WebSocket and waits for the `JoinResponse`.
async fn join_signal(url: &str, token: &str) -> (Ws, livekit_protocol::JoinResponse) {
    let (mut ws, _) = tokio_tungstenite::connect_async(rtc_url(url, token))
        .await
        .expect("LiveKit rejected the token");
    let join = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(msg) = ws.next().await {
            if let Message::Binary(bytes) = msg.unwrap() {
                let res = SignalResponse::decode(bytes.as_ref()).unwrap();
                if let Some(signal_response::Message::Join(join)) = res.message {
                    return join;
                }
            }
        }
        panic!("signal connection closed before JoinResponse");
    })
    .await
    .expect("timed out waiting for JoinResponse");
    (ws, join)
}

#[tokio::test]
async fn room_service_accepts_dev_keys() {
    let bin = require_livekit!();
    let livekit = LiveKitServer::start(&bin, &log_path("room-service"))
        .await
        .unwrap();
    let client = RoomClient::with_api_key(&livekit.http_url(), API_KEY, API_SECRET);

    client
        .create_room("sheeshcord-admin", CreateRoomOptions::default())
        .await
        .unwrap();
    let rooms = client.list_rooms(vec![]).await.unwrap();
    assert!(rooms.iter().any(|r| r.name == "sheeshcord-admin"));
}

#[tokio::test]
async fn token_server_jwt_joins_livekit_and_participants_are_listed() {
    let bin = require_livekit!();
    let livekit = LiveKitServer::start(&bin, &log_path("participants"))
        .await
        .unwrap();
    let tokens = TokenServer::start(&livekit.ws_url(), API_SECRET)
        .await
        .unwrap();
    let room = "sheeshcord-it";

    let health: Value = reqwest::get(format!("{}/health", tokens.url()))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health, json!({ "status": "ok" }));

    let ana = request_token(&tokens, room, "ana").await;
    let bia = request_token(&tokens, room, "bia").await;
    assert_eq!(ana.url, livekit.ws_url());

    let (_ws_ana, join_ana) = join_signal(&ana.url, &ana.token).await;
    let (_ws_bia, join_bia) = join_signal(&bia.url, &bia.token).await;
    assert_eq!(join_ana.room.unwrap().name, room);
    assert_eq!(
        join_bia.participant.unwrap().identity,
        identity_of(&bia.token)
    );

    let client = RoomClient::with_api_key(&livekit.http_url(), API_KEY, API_SECRET);
    let mut expected = vec![identity_of(&ana.token), identity_of(&bia.token)];
    expected.sort();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let participants = loop {
        let list = client.list_participants(room).await.unwrap();
        if list.len() == 2 || tokio::time::Instant::now() > deadline {
            break list;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let mut identities: Vec<_> = participants.iter().map(|p| p.identity.clone()).collect();
    identities.sort();
    assert_eq!(identities, expected);
    let mut names: Vec<_> = participants.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["ana", "bia"]);

    let rooms = client.list_rooms(vec![room.to_owned()]).await.unwrap();
    assert_eq!(rooms.len(), 1);
}

#[tokio::test]
async fn livekit_rejects_token_signed_with_wrong_secret() {
    let bin = require_livekit!();
    let livekit = LiveKitServer::start(&bin, &log_path("wrong-secret"))
        .await
        .unwrap();
    let tokens = TokenServer::start(&livekit.ws_url(), "wrongsecretwrongsecretwrongsecret00")
        .await
        .unwrap();

    let res = request_token(&tokens, "sheeshcord-it", "eve").await;
    let result = tokio_tungstenite::connect_async(rtc_url(&res.url, &res.token)).await;
    assert!(
        result.is_err(),
        "LiveKit accepted a token with a bad signature"
    );
}

#[tokio::test]
async fn token_server_returns_400_over_real_http() {
    let tokens = TokenServer::start("ws://127.0.0.1:7880", API_SECRET)
        .await
        .unwrap();
    let res = reqwest::Client::new()
        .post(format!("{}/token", tokens.url()))
        .json(&json!({ "room": "", "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}
