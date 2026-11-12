//! Headless load/quality test for Sheeshcord.
//!
//! Simulates N participants in one LiveKit room: one publishes a synthetic
//! screen share (test pattern) and the others watch it. Measures connection
//! time, received bitrate/FPS, packet loss and jitter, then evaluates the
//! results against configurable thresholds.

pub mod impairment;
pub mod metrics;
pub mod pattern;
pub mod runner;

pub use impairment::{CommandHook, Impairment, ImpairmentHook, NoopHook};
pub use metrics::{
    evaluate, summarize, InboundSample, LoadReport, PublisherMetrics, Summary, Thresholds,
    ViewerMetrics, Violation,
};
pub use pattern::TestPattern;
pub use runner::{run, Codec, RunConfig};
