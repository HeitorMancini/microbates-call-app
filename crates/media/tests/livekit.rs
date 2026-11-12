//! Two `LiveKitTransport` clients with synthetic media against a local `livekit-server`.
//! Skipped (with a message) when `livekit-server` is not installed.

use std::time::Duration;

use integration_tests::{find_livekit_server, LiveKitServer, TokenServer, API_SECRET};
use sheeshcord_core::{
    AudioSource, ScreenPreset, ScreenSource, Session, TokenProvider, Transport, TransportEvent,
};
use sheeshcord_media::{
    FakeAudio, FakeScreen, HttpTokenProvider, LiveKitTransport, MediaHub, FAKE_MONITOR_ID,
};

struct Client {
    hub: MediaHub,
    transport: LiveKitTransport,
    audio: FakeAudio,
    screen: FakeScreen,
    session: Session,
}

async fn join(tokens: &HttpTokenProvider, token_url: &str, name: &str) -> Client {
    let creds = tokens
        .fetch_token(token_url, "media-it", name)
        .await
        .expect("token");
    let hub = MediaHub::default();
    let transport = LiveKitTransport::new(hub.clone());
    let session = transport
        .connect(&creds.url, &creds.token)
        .await
        .expect("connect");
    Client {
        audio: FakeAudio::new(hub.clone()),
        screen: FakeScreen::new(hub.clone()),
        hub,
        transport,
        session,
    }
}

async fn wait_for(what: &str, limit: Duration, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + limit;
    while !check() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_clients_exchange_tone_and_test_pattern() {
    let Some(bin) = find_livekit_server() else {
        eprintln!("SKIPPED: livekit-server not found (set LIVEKIT_SERVER_BIN or add it to PATH)");
        return;
    };
    let log = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("media-livekit-server.log");
    let server = LiveKitServer::start(&bin, &log)
        .await
        .expect("start livekit-server");
    let token_server = TokenServer::start(&server.ws_url(), API_SECRET)
        .await
        .expect("start token-server");
    let tokens = HttpTokenProvider::default();

    let mut alice = join(&tokens, &token_server.url(), "alice").await;
    let mut bob = join(&tokens, &token_server.url(), "bob").await;

    // Alice was already in the room, so Bob sees her in the initial snapshot or right after.
    let alice_id = alice.session.local_identity.clone();
    let mut bob_sees_alice = bob.session.participants.iter().any(|p| p.id == alice_id);
    let mut alice_sees_bob = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !(bob_sees_alice && alice_sees_bob) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "participants not visible"
        );
        tokio::select! {
            Some(TransportEvent::ParticipantJoined(p)) = alice.session.events.recv() => {
                alice_sees_bob |= p.id == bob.session.local_identity;
            }
            Some(TransportEvent::ParticipantJoined(p)) = bob.session.events.recv() => {
                bob_sees_alice |= p.id == alice_id;
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    let mut previews = bob.hub.subscribe_previews();
    alice.audio.start().await.unwrap();
    bob.audio.start().await.unwrap();
    alice
        .screen
        .start(FAKE_MONITOR_ID, ScreenPreset::P720Fps30)
        .await
        .unwrap();
    alice
        .transport
        .publish_screen(ScreenPreset::P720Fps30)
        .await
        .unwrap();

    let limit = Duration::from_secs(20);
    let bob_hub = bob.hub.clone();
    wait_for("remote audio", limit, || {
        bob_hub.stats().remote_audio_samples > 0
    })
    .await;
    wait_for("remote video", limit, || {
        bob_hub.stats().remote_video_frames > 0
    })
    .await;
    wait_for("tone playback", limit, || {
        bob_hub.stats().played_samples > 0
    })
    .await;

    let frame = tokio::time::timeout(limit, async {
        loop {
            match previews.recv().await {
                Ok(f) if f.participant_id == alice_id => break f,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("preview channel closed: {e}"),
            }
        }
    })
    .await
    .expect("preview frame");
    assert!(frame.width > 0 && frame.width <= 1280);
    assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);

    let mut sharing_seen = false;
    while let Ok(event) = bob.session.events.try_recv() {
        if let TransportEvent::ScreenShareChanged { id, sharing: true } = event {
            sharing_seen |= id == alice_id;
        }
    }
    assert!(
        sharing_seen,
        "bob must be told that alice shares her screen"
    );

    let stats = bob_hub.stats();
    eprintln!("bob stats: {stats:?}");
    eprintln!("alice stats: {:?}", alice.hub.stats());

    alice.transport.unpublish_screen().await.unwrap();
    alice.screen.stop().await;
    alice.audio.stop().await;
    bob.audio.stop().await;
    alice.transport.disconnect().await;
    bob.transport.disconnect().await;
}
