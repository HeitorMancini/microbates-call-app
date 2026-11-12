//! Pure metric math and pass/fail evaluation (no LiveKit types, fully unit-tested).

use serde::{Deserialize, Serialize};

/// A cumulative snapshot of one inbound video stream (from WebRTC `inbound-rtp`
/// stats plus the frames counted by our own sink).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InboundSample {
    /// Seconds since an arbitrary monotonic origin.
    pub at_secs: f64,
    pub bytes_received: u64,
    pub packets_received: u64,
    /// Cumulative lost packets as reported by RTCP (may go negative with duplicates).
    pub packets_lost: i64,
    /// Frames delivered to the local sink.
    pub frames_received: u64,
    /// Current inter-arrival jitter estimate, in seconds.
    pub jitter_secs: f64,
    pub frame_width: u32,
    pub frame_height: u32,
}

/// Rates computed between two cumulative samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowRates {
    pub bitrate_kbps: f64,
    pub fps: f64,
    pub loss_pct: f64,
}

/// Computes bitrate, FPS and loss between `start` and `end`.
/// Returns zeros if the window is empty or the counters went backwards.
pub fn window_rates(start: &InboundSample, end: &InboundSample) -> WindowRates {
    let secs = end.at_secs - start.at_secs;
    if secs <= 0.0 {
        return WindowRates::default();
    }
    let bytes = end.bytes_received.saturating_sub(start.bytes_received);
    let frames = end.frames_received.saturating_sub(start.frames_received);
    let received = end.packets_received.saturating_sub(start.packets_received) as f64;
    let lost = (end.packets_lost - start.packets_lost).max(0) as f64;
    let loss_pct = if received + lost > 0.0 {
        lost / (received + lost) * 100.0
    } else {
        0.0
    };
    WindowRates {
        bitrate_kbps: bytes as f64 * 8.0 / 1000.0 / secs,
        fps: frames as f64 / secs,
        loss_pct,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewerMetrics {
    pub identity: String,
    pub connect_ms: f64,
    /// Time from viewer connect until the first decoded frame, if any.
    pub first_frame_ms: Option<f64>,
    /// `false` if the viewer never got the screen track.
    pub subscribed: bool,
    pub bitrate_kbps: f64,
    pub fps: f64,
    pub loss_pct: f64,
    pub jitter_ms: f64,
    pub frame_width: u32,
    pub frame_height: u32,
}

impl ViewerMetrics {
    pub fn from_window(
        identity: String,
        connect_ms: f64,
        first_frame_ms: Option<f64>,
        start: &InboundSample,
        end: &InboundSample,
    ) -> Self {
        let rates = window_rates(start, end);
        Self {
            identity,
            connect_ms,
            first_frame_ms,
            subscribed: true,
            bitrate_kbps: rates.bitrate_kbps,
            fps: rates.fps,
            loss_pct: rates.loss_pct,
            jitter_ms: start.jitter_secs.max(end.jitter_secs) * 1000.0,
            frame_width: end.frame_width,
            frame_height: end.frame_height,
        }
    }

    pub fn unsubscribed(identity: String, connect_ms: f64) -> Self {
        Self {
            identity,
            connect_ms,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PublisherMetrics {
    pub identity: String,
    pub connect_ms: f64,
    pub target_bitrate_kbps: f64,
    pub sent_bitrate_kbps: f64,
    pub frames_captured: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub viewers_expected: usize,
    pub viewers_receiving: usize,
    pub min_bitrate_kbps: f64,
    pub avg_bitrate_kbps: f64,
    pub min_fps: f64,
    pub avg_fps: f64,
    pub max_loss_pct: f64,
    pub max_jitter_ms: f64,
    pub max_connect_ms: f64,
    pub max_first_frame_ms: Option<f64>,
}

/// Aggregates viewer metrics. Viewers that never subscribed count as 0 kbps / 0 fps.
pub fn summarize(viewers: &[ViewerMetrics], publisher_connect_ms: f64) -> Summary {
    let n = viewers.len();
    let fold_min = |f: fn(&ViewerMetrics) -> f64| {
        if n == 0 {
            0.0
        } else {
            viewers.iter().map(f).fold(f64::INFINITY, f64::min)
        }
    };
    let avg = |f: fn(&ViewerMetrics) -> f64| {
        if n == 0 {
            0.0
        } else {
            viewers.iter().map(f).sum::<f64>() / n as f64
        }
    };
    let max = |f: fn(&ViewerMetrics) -> f64| viewers.iter().map(f).fold(0.0, f64::max);
    let first_frames: Vec<f64> = viewers.iter().filter_map(|v| v.first_frame_ms).collect();
    Summary {
        viewers_expected: n,
        viewers_receiving: viewers
            .iter()
            .filter(|v| v.subscribed && v.fps > 0.0)
            .count(),
        min_bitrate_kbps: fold_min(|v| v.bitrate_kbps),
        avg_bitrate_kbps: avg(|v| v.bitrate_kbps),
        min_fps: fold_min(|v| v.fps),
        avg_fps: avg(|v| v.fps),
        max_loss_pct: max(|v| v.loss_pct),
        max_jitter_ms: max(|v| v.jitter_ms),
        max_connect_ms: max(|v| v.connect_ms).max(publisher_connect_ms),
        max_first_frame_ms: (!first_frames.is_empty())
            .then(|| first_frames.iter().copied().fold(0.0, f64::max)),
    }
}

/// Pass/fail limits. Applied to the worst viewer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    pub min_bitrate_kbps: f64,
    pub min_fps: f64,
    pub max_loss_pct: f64,
    pub max_jitter_ms: f64,
    pub max_connect_ms: f64,
}

impl Default for Thresholds {
    /// Targets for 1080p30 screen share through the SFU.
    fn default() -> Self {
        Self {
            min_bitrate_kbps: 1000.0,
            min_fps: 24.0,
            max_loss_pct: 2.0,
            max_jitter_ms: 30.0,
            max_connect_ms: 3000.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    pub metric: String,
    pub actual: f64,
    pub limit: f64,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {:.2} (limite {:.2})",
            self.metric, self.actual, self.limit
        )
    }
}

pub fn evaluate(summary: &Summary, t: &Thresholds) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut check = |metric: &str, actual: f64, limit: f64, ok: bool| {
        if !ok {
            out.push(Violation {
                metric: metric.to_owned(),
                actual,
                limit,
            });
        }
    };
    let expected = summary.viewers_expected as f64;
    let receiving = summary.viewers_receiving as f64;
    check(
        "viewers_receiving",
        receiving,
        expected,
        receiving >= expected,
    );
    check(
        "min_bitrate_kbps",
        summary.min_bitrate_kbps,
        t.min_bitrate_kbps,
        summary.min_bitrate_kbps >= t.min_bitrate_kbps,
    );
    check(
        "min_fps",
        summary.min_fps,
        t.min_fps,
        summary.min_fps >= t.min_fps,
    );
    check(
        "max_loss_pct",
        summary.max_loss_pct,
        t.max_loss_pct,
        summary.max_loss_pct <= t.max_loss_pct,
    );
    check(
        "max_jitter_ms",
        summary.max_jitter_ms,
        t.max_jitter_ms,
        summary.max_jitter_ms <= t.max_jitter_ms,
    );
    check(
        "max_connect_ms",
        summary.max_connect_ms,
        t.max_connect_ms,
        summary.max_connect_ms <= t.max_connect_ms,
    );
    out
}

/// Full JSON report written by the CLI.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadReport {
    pub room: String,
    pub participants: usize,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub measured_secs: f64,
    pub impairment: Option<crate::Impairment>,
    pub publisher: PublisherMetrics,
    pub viewers: Vec<ViewerMetrics>,
    pub summary: Summary,
    pub thresholds: Thresholds,
    pub violations: Vec<Violation>,
    pub passed: bool,
}

impl LoadReport {
    pub fn human_summary(&self) -> String {
        use std::fmt::Write as _;
        let s = &self.summary;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Sala '{}': {} participantes, tela {}x{}@{} por {:.1}s",
            self.room, self.participants, self.width, self.height, self.fps, self.measured_secs
        );
        let _ = writeln!(
            out,
            "Publicador: conexão {:.0} ms, alvo {:.0} kbps, enviado {:.0} kbps, {} quadros",
            self.publisher.connect_ms,
            self.publisher.target_bitrate_kbps,
            self.publisher.sent_bitrate_kbps,
            self.publisher.frames_captured
        );
        for v in &self.viewers {
            let _ = writeln!(
                out,
                "  {:<16} {:>7.0} kbps {:>5.1} fps perda {:>4.1}% jitter {:>5.1} ms conexão {:>5.0} ms {}x{}",
                v.identity,
                v.bitrate_kbps,
                v.fps,
                v.loss_pct,
                v.jitter_ms,
                v.connect_ms,
                v.frame_width,
                v.frame_height
            );
        }
        let _ = writeln!(
            out,
            "Resumo: {}/{} recebendo, bitrate min/média {:.0}/{:.0} kbps, fps min/média {:.1}/{:.1}, perda máx {:.2}%, jitter máx {:.1} ms, conexão máx {:.0} ms",
            s.viewers_receiving,
            s.viewers_expected,
            s.min_bitrate_kbps,
            s.avg_bitrate_kbps,
            s.min_fps,
            s.avg_fps,
            s.max_loss_pct,
            s.max_jitter_ms,
            s.max_connect_ms
        );
        if self.passed {
            let _ = writeln!(out, "RESULTADO: APROVADO");
        } else {
            let _ = writeln!(out, "RESULTADO: REPROVADO");
            for v in &self.violations {
                let _ = writeln!(out, "  - {v}");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: f64, bytes: u64, pkts: u64, lost: i64, frames: u64) -> InboundSample {
        InboundSample {
            at_secs: at,
            bytes_received: bytes,
            packets_received: pkts,
            packets_lost: lost,
            frames_received: frames,
            jitter_secs: 0.0,
            frame_width: 1920,
            frame_height: 1080,
        }
    }

    fn viewer(kbps: f64, fps: f64, loss: f64, jitter: f64, connect: f64) -> ViewerMetrics {
        ViewerMetrics {
            identity: "v".into(),
            connect_ms: connect,
            first_frame_ms: Some(100.0),
            subscribed: true,
            bitrate_kbps: kbps,
            fps,
            loss_pct: loss,
            jitter_ms: jitter,
            frame_width: 1920,
            frame_height: 1080,
        }
    }

    #[test]
    fn rates_over_window() {
        let a = sample(10.0, 1_000_000, 1000, 5, 300);
        let b = sample(20.0, 4_750_000, 1990, 15, 600);
        let r = window_rates(&a, &b);
        assert!((r.bitrate_kbps - 3000.0).abs() < 1e-9);
        assert!((r.fps - 30.0).abs() < 1e-9);
        // 10 lost of 1000 total packets in the window.
        assert!((r.loss_pct - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rates_handle_empty_or_backwards_windows() {
        let a = sample(5.0, 100, 10, 3, 10);
        assert_eq!(window_rates(&a, &a), WindowRates::default());
        let earlier = sample(4.0, 0, 0, 0, 0);
        assert_eq!(window_rates(&a, &earlier), WindowRates::default());
        // Counter reset / RTCP correction must not produce negative values.
        let reset = sample(6.0, 50, 5, 1, 5);
        let r = window_rates(&a, &reset);
        assert_eq!((r.bitrate_kbps, r.fps, r.loss_pct), (0.0, 0.0, 0.0));
    }

    #[test]
    fn viewer_jitter_uses_worst_sample_in_ms() {
        let mut a = sample(0.0, 0, 0, 0, 0);
        let mut b = sample(1.0, 1000, 10, 0, 30);
        a.jitter_secs = 0.012;
        b.jitter_secs = 0.004;
        let v = ViewerMetrics::from_window("x".into(), 50.0, None, &a, &b);
        assert!((v.jitter_ms - 12.0).abs() < 1e-9);
        assert!(v.subscribed);
    }

    #[test]
    fn summary_takes_worst_and_average() {
        let vs = vec![
            viewer(3000.0, 30.0, 0.5, 5.0, 400.0),
            viewer(2000.0, 20.0, 1.5, 15.0, 900.0),
        ];
        let s = summarize(&vs, 1200.0);
        assert_eq!(s.viewers_expected, 2);
        assert_eq!(s.viewers_receiving, 2);
        assert_eq!(s.min_bitrate_kbps, 2000.0);
        assert_eq!(s.avg_bitrate_kbps, 2500.0);
        assert_eq!(s.min_fps, 20.0);
        assert_eq!(s.avg_fps, 25.0);
        assert_eq!(s.max_loss_pct, 1.5);
        assert_eq!(s.max_jitter_ms, 15.0);
        assert_eq!(s.max_connect_ms, 1200.0);
        assert_eq!(s.max_first_frame_ms, Some(100.0));
    }

    #[test]
    fn summary_of_no_viewers_is_zero() {
        let s = summarize(&[], 10.0);
        assert_eq!(s.min_bitrate_kbps, 0.0);
        assert_eq!(s.min_fps, 0.0);
        assert_eq!(s.max_first_frame_ms, None);
    }

    #[test]
    fn healthy_run_passes() {
        let s = summarize(&[viewer(2500.0, 29.5, 0.1, 4.0, 300.0)], 300.0);
        assert!(evaluate(&s, &Thresholds::default()).is_empty());
    }

    #[test]
    fn each_threshold_is_reported() {
        let mut vs = vec![viewer(500.0, 10.0, 5.0, 80.0, 5000.0)];
        vs.push(ViewerMetrics::unsubscribed("missing".into(), 100.0));
        let s = summarize(&vs, 0.0);
        let names: Vec<_> = evaluate(&s, &Thresholds::default())
            .into_iter()
            .map(|v| v.metric)
            .collect();
        assert_eq!(
            names,
            [
                "viewers_receiving",
                "min_bitrate_kbps",
                "min_fps",
                "max_loss_pct",
                "max_jitter_ms",
                "max_connect_ms"
            ]
        );
    }

    #[test]
    fn thresholds_are_inclusive() {
        let t = Thresholds::default();
        let s = summarize(
            &[viewer(
                t.min_bitrate_kbps,
                t.min_fps,
                t.max_loss_pct,
                t.max_jitter_ms,
                t.max_connect_ms,
            )],
            0.0,
        );
        assert!(evaluate(&s, &t).is_empty());
    }
}
