//! Two `LiveKitTransport` clients with synthetic media against a local `livekit-server`.
//! Skipped (with a message) when `livekit-server` is not installed.

use std::time::Duration;

use integration_tests::{find_livekit_server, LiveKitServer, TokenServer, API_SECRET};
use sheeshcord_core::{
    AudioSource, JoinParams, ScreenPreset, ScreenSource, Session, TokenProvider, Transport,
    TransportEvent,
};
use sheeshcord_media::{
    FakeAudio, FakeScreen, HttpTokenProvider, LiveKitTransport, MediaHub, TransportOptions,
    FAKE_MONITOR_ID,
};

struct Client {
    hub: MediaHub,
    transport: LiveKitTransport,
    audio: FakeAudio,
    screen: FakeScreen,
    session: Session,
}

async fn join(
    tokens: &HttpTokenProvider,
    token_url: &str,
    name: &str,
    hardware_encoder: bool,
) -> Client {
    let creds = tokens
        .fetch_token(&JoinParams {
            server_url: token_url.to_owned(),
            room: "media-it".to_owned(),
            name: name.to_owned(),
            password: String::new(),
        })
        .await
        .expect("token");
    let hub = MediaHub::default();
    let transport = LiveKitTransport::with_options(
        hub.clone(),
        TransportOptions {
            subscribe_video: true,
            hardware_encoder,
            ..Default::default()
        },
    );
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

    let mut alice = join(&tokens, &token_server.url(), "alice", false).await;
    let mut bob = join(&tokens, &token_server.url(), "bob", false).await;

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
    let info = alice
        .transport
        .screen_encoder()
        .await
        .expect("screen encoder info");
    assert!(
        !info.hardware,
        "hardware_encoder=false must encode in software: {info:?}"
    );

    alice.transport.unpublish_screen().await.unwrap();
    alice.screen.stop().await;
    alice.audio.stop().await;
    bob.audio.stop().await;
    alice.transport.disconnect().await;
    bob.transport.disconnect().await;
}

/// Shares alice's fake screen to bob with the GPU encoder enabled. Set `MEDIA_EXPECT_HW`
/// on machines with a hardware encoder to require that it is used.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn screen_share_uses_gpu_encoder_when_available() {
    let Some(bin) = find_livekit_server() else {
        eprintln!("SKIPPED: livekit-server not found (set LIVEKIT_SERVER_BIN or add it to PATH)");
        return;
    };
    let log = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("media-gpu-server.log");
    let server = LiveKitServer::start(&bin, &log)
        .await
        .expect("start livekit-server");
    let token_server = TokenServer::start(&server.ws_url(), API_SECRET)
        .await
        .expect("start token-server");
    let tokens = HttpTokenProvider::default();
    let alice = join(&tokens, &token_server.url(), "alice", true).await;
    let bob = join(&tokens, &token_server.url(), "bob", true).await;

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
    let bob_hub = bob.hub.clone();
    wait_for("remote video", Duration::from_secs(20), || {
        bob_hub.stats().remote_video_frames > 30
    })
    .await;

    let info = alice
        .transport
        .screen_encoder()
        .await
        .expect("screen encoder info");
    eprintln!("screen encoder: {info:?}");
    if std::env::var_os("MEDIA_EXPECT_HW").is_some() {
        assert!(info.hardware, "expected a GPU encoder, got {info:?}");
    }

    alice.transport.unpublish_screen().await.unwrap();
    assert!(alice.transport.screen_encoder().await.is_none());
    alice.screen.stop().await;
    alice.transport.disconnect().await;
    bob.transport.disconnect().await;
}

/// Measures the frame rate bob receives from a 1080p60 GPU-encoded share. Run with
/// `--ignored` on machines with a hardware encoder.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "performance check; needs a GPU encoder"]
async fn screen_share_sustains_1080p60_on_gpu() {
    let Some(bin) = find_livekit_server() else {
        eprintln!("SKIPPED: livekit-server not found (set LIVEKIT_SERVER_BIN or add it to PATH)");
        return;
    };
    let log = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("media-perf-server.log");
    let server = LiveKitServer::start(&bin, &log)
        .await
        .expect("start livekit-server");
    let token_server = TokenServer::start(&server.ws_url(), API_SECRET)
        .await
        .expect("start token-server");
    let tokens = HttpTokenProvider::default();
    let alice = join(&tokens, &token_server.url(), "alice", true).await;
    let bob = join(&tokens, &token_server.url(), "bob", true).await;

    alice
        .screen
        .start(FAKE_MONITOR_ID, ScreenPreset::P1080Fps60)
        .await
        .unwrap();
    alice
        .transport
        .publish_screen(ScreenPreset::P1080Fps60)
        .await
        .unwrap();
    let info = alice
        .transport
        .screen_encoder()
        .await
        .expect("encoder info");
    if !info.hardware {
        eprintln!("SKIPPED: no GPU encoder on this machine ({info:?})");
    } else {
        let bob_hub = bob.hub.clone();
        wait_for("remote video", Duration::from_secs(20), || {
            bob_hub.stats().remote_video_frames > 0
        })
        .await;
        let window = Duration::from_secs(10);
        let before = bob_hub.stats().remote_video_frames;
        tokio::time::sleep(window).await;
        let frames = bob_hub.stats().remote_video_frames - before;
        let fps = frames as f64 / window.as_secs_f64();
        eprintln!("{info:?}: {fps:.1} fps at 1080p60");
        assert!(fps >= 50.0, "{info:?}: {fps:.1} fps");
    }

    alice.transport.unpublish_screen().await.unwrap();
    alice.screen.stop().await;
    alice.transport.disconnect().await;
    bob.transport.disconnect().await;
}

/// When the GPU encoder dies mid-share the screen is republished in software and bob keeps
/// receiving video.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hardware_failure_falls_back_to_software() {
    let Some(bin) = find_livekit_server() else {
        eprintln!("SKIPPED: livekit-server not found (set LIVEKIT_SERVER_BIN or add it to PATH)");
        return;
    };
    let log = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("media-hwfail-server.log");
    let server = LiveKitServer::start(&bin, &log)
        .await
        .expect("start livekit-server");
    let token_server = TokenServer::start(&server.ws_url(), API_SECRET)
        .await
        .expect("start token-server");
    let tokens = HttpTokenProvider::default();
    let alice = join(&tokens, &token_server.url(), "alice", true).await;
    let bob = join(&tokens, &token_server.url(), "bob", true).await;

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
    let info = alice
        .transport
        .screen_encoder()
        .await
        .expect("encoder info");
    if !info.hardware {
        eprintln!("SKIPPED: no GPU encoder on this machine ({info:?})");
    } else {
        let bob_hub = bob.hub.clone();
        wait_for("remote video", Duration::from_secs(20), || {
            bob_hub.stats().remote_video_frames > 30
        })
        .await;
        alice.transport.fail_hardware_encoder_for_test().await;
        let before = bob_hub.stats().remote_video_frames;
        wait_for("video after fallback", Duration::from_secs(15), || {
            bob_hub.stats().remote_video_frames > before + 30
        })
        .await;
        let info = alice
            .transport
            .screen_encoder()
            .await
            .expect("encoder info");
        eprintln!("after fallback: {info:?}");
        assert!(!info.hardware, "still on the GPU: {info:?}");
    }

    alice.transport.unpublish_screen().await.unwrap();
    alice.screen.stop().await;
    alice.transport.disconnect().await;
    bob.transport.disconnect().await;
}
