//! Smoke test: 3 clients (1 publisher + 2 viewers) against a local livekit-server.

use std::path::PathBuf;
use std::time::Duration;

use integration_tests::{find_livekit_server, LiveKitServer, API_KEY, API_SECRET};
use load_test::{run, NoopHook, RunConfig, Thresholds};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_clients_receive_synthetic_screen() {
    let Some(bin) = find_livekit_server() else {
        eprintln!(
            "SKIPPED: livekit-server not found in PATH, LIVEKIT_SERVER_BIN or %USERPROFILE%\\.local\\bin"
        );
        return;
    };
    let log = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("livekit-load-smoke.log");
    let server = LiveKitServer::start(&bin, &log)
        .await
        .expect("start livekit");

    let cfg = RunConfig {
        url: server.ws_url(),
        api_key: API_KEY.into(),
        api_secret: API_SECRET.into(),
        room: "load-smoke".into(),
        participants: 3,
        width: 640,
        height: 360,
        fps: 15,
        bitrate_kbps: 600,
        warmup: Duration::from_secs(2),
        duration: Duration::from_secs(4),
        ..RunConfig::default()
    };
    // Lenient limits: this checks the pipeline works, not real-world quality.
    let thresholds = Thresholds {
        min_bitrate_kbps: 30.0,
        min_fps: 5.0,
        max_loss_pct: 5.0,
        max_jitter_ms: 200.0,
        max_connect_ms: 10_000.0,
    };
    let report = run(&cfg, &thresholds, None, &NoopHook)
        .await
        .expect("load run");
    println!("{}", report.human_summary());

    assert_eq!(report.viewers.len(), 2);
    assert!(report.publisher.frames_captured > 0);
    assert!(report.viewers.iter().all(|v| v.subscribed));
    assert!(report.passed, "violations: {:?}", report.violations);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["summary"]["viewers_receiving"], 2);
}
