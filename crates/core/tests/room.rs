use std::sync::Arc;
use std::time::Duration;

use sheeshcord_core::mock::{MockAudio, MockScreen, MockTokenProvider, MockTransport};
use sheeshcord_core::{
    Backoff, CoreError, Dependencies, DeviceKind, JoinParams, Participant, RemoteParticipant,
    RoomConfig, RoomController, RoomEvent, RoomState, RoomStatus, ScreenPreset, TransportEvent,
};
use tokio::sync::broadcast;
use tokio::time::{timeout, Instant};

const BACKOFF: Backoff = Backoff {
    initial: Duration::from_millis(100),
    max: Duration::from_secs(1),
    max_attempts: 3,
};

struct Harness {
    room: RoomController,
    events: broadcast::Receiver<RoomEvent>,
    tokens: Arc<MockTokenProvider>,
    transport: Arc<MockTransport>,
    audio: Arc<MockAudio>,
    screen: Arc<MockScreen>,
}

fn harness() -> Harness {
    let tokens = Arc::new(MockTokenProvider::default());
    let transport = Arc::new(MockTransport::default());
    let audio = Arc::new(MockAudio::default());
    let screen = Arc::new(MockScreen::default());
    let room = RoomController::new(
        Dependencies {
            tokens: tokens.clone(),
            transport: transport.clone(),
            audio: audio.clone(),
            screen: screen.clone(),
        },
        RoomConfig { backoff: BACKOFF },
    );
    let events = room.subscribe();
    Harness {
        room,
        events,
        tokens,
        transport,
        audio,
        screen,
    }
}

fn params() -> JoinParams {
    JoinParams {
        server_url: "http://26.1.2.3:7881".into(),
        room: "geral".into(),
        name: "heitor".into(),
        password: String::new(),
    }
}

fn remote(id: &str) -> RemoteParticipant {
    RemoteParticipant {
        id: id.into(),
        name: id.to_uppercase(),
        ..Default::default()
    }
}

fn ids(participants: &[Participant]) -> Vec<&str> {
    participants.iter().map(|p| p.id.as_str()).collect()
}

impl Harness {
    async fn next(&mut self) -> RoomEvent {
        timeout(Duration::from_secs(30), self.events.recv())
            .await
            .expect("timed out waiting for room event")
            .expect("event channel closed")
    }

    async fn wait_state(&mut self, status: RoomStatus) -> RoomState {
        loop {
            if let RoomEvent::State(state) = self.next().await {
                if state.status == status {
                    return state;
                }
            }
        }
    }

    async fn wait_participants(
        &mut self,
        pred: impl Fn(&[Participant]) -> bool,
    ) -> Vec<Participant> {
        loop {
            if let RoomEvent::Participants(list) = self.next().await {
                if pred(&list) {
                    return list;
                }
            }
        }
    }

    fn drain_states(&mut self) -> Vec<RoomStatus> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if let RoomEvent::State(s) = event {
                out.push(s.status);
            }
        }
        out
    }
}

#[tokio::test]
async fn join_connects_and_emits_states() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();

    assert_eq!(
        h.drain_states(),
        [RoomStatus::Connecting, RoomStatus::Connected]
    );
    let snapshot = h.room.snapshot();
    assert_eq!(snapshot.state, RoomState::new(RoomStatus::Connected));
    let local = &snapshot.participants[0];
    assert!(local.is_local);
    assert_eq!(local.name, "heitor");
    assert!(h.audio.state().started);
    assert_eq!(
        h.transport.last_token().unwrap(),
        "mock-token-1-geral-heitor"
    );
}

#[tokio::test]
async fn token_failure_sets_error_and_allows_retry() {
    let mut h = harness();
    h.tokens.set_error(Some("401 unauthorized"));

    let err = h.room.join(params()).await.unwrap_err();
    assert!(matches!(err, CoreError::Token(ref m) if m.contains("401")));
    let state = h.wait_state(RoomStatus::Error).await;
    assert!(state.error.unwrap().contains("401"));
    assert_eq!(h.transport.connect_calls(), 0);
    assert!(!h.audio.state().started);

    h.tokens.set_error(None);
    h.room.join(params()).await.unwrap();
    assert_eq!(h.room.snapshot().state.status, RoomStatus::Connected);
}

#[tokio::test]
async fn connect_failure_sets_error() {
    let mut h = harness();
    h.transport.fail_next_connects(1);
    let err = h.room.join(params()).await.unwrap_err();
    assert!(matches!(err, CoreError::Transport(_)));
    h.wait_state(RoomStatus::Error).await;
}

#[tokio::test]
async fn audio_failure_disconnects_transport() {
    let h = harness();
    h.audio.set_fail_start(true);
    let err = h.room.join(params()).await.unwrap_err();
    assert!(matches!(err, CoreError::Audio(_)));
    assert!(!h.transport.is_connected());
    assert_eq!(h.room.snapshot().state.status, RoomStatus::Error);
}

#[tokio::test]
async fn joining_twice_is_rejected() {
    let h = harness();
    h.room.join(params()).await.unwrap();
    assert_eq!(h.room.join(params()).await, Err(CoreError::AlreadyActive));
}

#[tokio::test]
async fn participants_join_update_and_leave() {
    let mut h = harness();
    h.transport.set_initial_participants(vec![remote("bob")]);
    h.room.join(params()).await.unwrap();
    assert_eq!(ids(&h.room.snapshot().participants), ["local-1", "bob"]);

    h.transport
        .emit(TransportEvent::ParticipantJoined(remote("carol")))
        .await;
    h.wait_participants(|p| p.len() == 3).await;

    h.transport
        .emit(TransportEvent::MuteChanged {
            id: "carol".into(),
            muted: true,
        })
        .await;
    h.transport
        .emit(TransportEvent::ActiveSpeakers(vec![
            "bob".into(),
            "local-1".into(),
        ]))
        .await;
    let list = h.wait_participants(|p| p.iter().any(|x| x.speaking)).await;
    assert!(list.iter().find(|p| p.id == "carol").unwrap().muted);
    let speaking: Vec<_> = list
        .iter()
        .filter(|p| p.speaking)
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(speaking, ["local-1", "bob"]);

    h.transport
        .emit(TransportEvent::ScreenShareChanged {
            id: "bob".into(),
            sharing: true,
        })
        .await;
    h.transport
        .emit(TransportEvent::ParticipantLeft { id: "carol".into() })
        .await;
    let list = h.wait_participants(|p| p.len() == 2).await;
    assert_eq!(ids(&list), ["local-1", "bob"]);
    assert!(list[1].sharing_screen);
}

#[tokio::test]
async fn remote_events_cannot_remove_local_participant() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();
    h.transport
        .emit(TransportEvent::ParticipantLeft {
            id: "local-1".into(),
        })
        .await;
    h.transport
        .emit(TransportEvent::ParticipantJoined(remote("x")))
        .await;
    let list = h.wait_participants(|p| p.len() == 2).await;
    assert_eq!(ids(&list), ["local-1", "x"]);
}

#[tokio::test]
async fn mute_and_deafen_propagate() {
    let h = harness();
    h.room.join(params()).await.unwrap();

    h.room.set_muted(true).await.unwrap();
    assert_eq!(h.transport.mic_muted(), Some(true));
    assert!(h.audio.state().capture_muted);
    assert!(h.room.snapshot().participants[0].muted);

    h.room.set_muted(false).await.unwrap();
    h.room.set_deafened(true).await.unwrap();
    assert!(h.audio.state().playback_muted);
    assert_eq!(
        h.transport.mic_muted(),
        Some(true),
        "deafen also mutes the mic"
    );
    assert!(h.room.snapshot().participants[0].muted);

    h.room.set_deafened(false).await.unwrap();
    assert!(!h.audio.state().playback_muted);
    assert_eq!(h.transport.mic_muted(), Some(false));
    let snapshot = h.room.snapshot();
    assert!(!snapshot.muted && !snapshot.deafened);
}

#[tokio::test]
async fn mute_before_join_is_applied_on_connect() {
    let h = harness();
    h.room.set_muted(true).await.unwrap();
    assert_eq!(h.transport.mic_muted(), None);
    assert!(h.room.snapshot().muted);

    h.room.join(params()).await.unwrap();
    assert_eq!(h.transport.mic_muted(), Some(true));
    assert!(h.room.snapshot().participants[0].muted);
}

#[tokio::test(start_paused = true)]
async fn reconnects_after_connection_drop() {
    let mut h = harness();
    h.transport.set_initial_participants(vec![remote("bob")]);
    h.room.join(params()).await.unwrap();
    h.room.set_muted(true).await.unwrap();

    h.transport.drop_connection("network down").await;
    h.wait_state(RoomStatus::Reconnecting).await;
    assert_eq!(ids(&h.room.snapshot().participants), ["local-1"]);

    h.wait_state(RoomStatus::Connected).await;
    assert_eq!(h.transport.connect_calls(), 2);
    assert_eq!(h.tokens.calls(), 2, "a fresh token is fetched on reconnect");
    assert_eq!(ids(&h.room.snapshot().participants), ["local-2", "bob"]);
    assert_eq!(
        h.transport.mic_muted(),
        Some(true),
        "mute survives reconnect"
    );
}

#[tokio::test(start_paused = true)]
async fn reconnect_uses_exponential_backoff() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();
    h.transport.fail_next_connects(2);

    h.transport.drop_connection("network down").await;
    h.wait_state(RoomStatus::Reconnecting).await;
    let started = Instant::now();
    h.wait_state(RoomStatus::Connected).await;

    assert_eq!(started.elapsed(), Duration::from_millis(100 + 200 + 400));
    assert_eq!(h.transport.connect_calls(), 4);
}

#[tokio::test(start_paused = true)]
async fn closed_event_channel_triggers_reconnect() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();
    h.transport.close_session();
    h.wait_state(RoomStatus::Reconnecting).await;
    h.wait_state(RoomStatus::Connected).await;
}

#[tokio::test(start_paused = true)]
async fn gives_up_after_max_attempts() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();
    h.transport.fail_next_connects(100);

    h.transport.drop_connection("network down").await;
    let state = h.wait_state(RoomStatus::Error).await;
    assert!(state.error.unwrap().contains("3 attempts"));
    assert_eq!(h.transport.connect_calls(), 1 + BACKOFF.max_attempts);
    assert!(!h.audio.state().started);
    assert!(h.room.snapshot().participants.is_empty());
}

#[tokio::test(start_paused = true)]
async fn token_failure_during_reconnect_is_retried() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();
    h.tokens.set_error(Some("token server down"));

    h.transport.drop_connection("network down").await;
    h.wait_state(RoomStatus::Reconnecting).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    h.tokens.set_error(None);
    h.wait_state(RoomStatus::Connected).await;
    assert_eq!(h.transport.connect_calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn leave_cancels_reconnection() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();
    h.transport.fail_next_connects(100);
    h.transport.drop_connection("network down").await;
    h.wait_state(RoomStatus::Reconnecting).await;

    h.room.leave().await.unwrap();
    h.wait_state(RoomStatus::Disconnected).await;
    let calls = h.transport.connect_calls();
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(h.transport.connect_calls(), calls);
}

#[tokio::test]
async fn leave_disconnects_everything() {
    let mut h = harness();
    h.room.join(params()).await.unwrap();
    h.room
        .start_screen_share("monitor-0", ScreenPreset::P1080Fps60)
        .await
        .unwrap();

    h.room.leave().await.unwrap();
    h.wait_state(RoomStatus::Disconnected).await;
    assert_eq!(h.transport.disconnect_calls(), 1);
    assert!(!h.audio.state().started);
    assert_eq!(h.screen.active(), None);
    assert!(h.room.snapshot().participants.is_empty());

    h.room.leave().await.unwrap();
    assert_eq!(h.transport.disconnect_calls(), 1, "leave is idempotent");
}

#[tokio::test]
async fn screen_share_requires_connection() {
    let h = harness();
    let err = h
        .room
        .start_screen_share("monitor-0", ScreenPreset::P720Fps30)
        .await
        .unwrap_err();
    assert_eq!(err, CoreError::NotConnected);
}

#[tokio::test]
async fn screen_share_start_and_stop() {
    let h = harness();
    h.room.join(params()).await.unwrap();

    let sources = h.room.list_screen_sources().await.unwrap();
    h.room
        .start_screen_share(&sources[1].id, ScreenPreset::P1080Fps30)
        .await
        .unwrap();
    assert_eq!(
        h.screen.active(),
        Some(("window-42".into(), ScreenPreset::P1080Fps30))
    );
    assert_eq!(
        h.transport.published_screen(),
        Some(ScreenPreset::P1080Fps30)
    );
    assert!(h.room.snapshot().participants[0].sharing_screen);

    h.room.stop_screen_share().await.unwrap();
    assert_eq!(h.screen.active(), None);
    assert_eq!(h.transport.published_screen(), None);
    assert!(!h.room.snapshot().participants[0].sharing_screen);
}

#[tokio::test]
async fn invalid_screen_source_is_reported() {
    let h = harness();
    h.room.join(params()).await.unwrap();
    let err = h
        .room
        .start_screen_share("nope", ScreenPreset::P720Fps30)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Screen(_)));
    assert_eq!(h.transport.published_screen(), None);
}

#[tokio::test]
async fn audio_devices_are_delegated() {
    let h = harness();
    let devices = h.room.list_audio_devices().await.unwrap();
    h.room
        .select_audio_device(DeviceKind::Input, &devices.inputs[0].id)
        .await
        .unwrap();
    assert_eq!(h.audio.state().input.as_deref(), Some("mic-1"));
    assert!(h
        .room
        .select_audio_device(DeviceKind::Output, "missing")
        .await
        .is_err());
}
