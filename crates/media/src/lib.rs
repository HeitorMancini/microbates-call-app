//! Real media implementations of the `sheeshcord_core` traits.
//!
//! * [`LiveKitTransport`]: LiveKit Rust SDK (libwebrtc: Opus, VP8/H264, APM).
//! * [`HttpTokenProvider`]: `POST /token` on the Sheeshcord token server.
//! * [`CpalAudio`]: microphone/speakers via `cpal` + WebRTC audio processing.
//! * [`WgcScreen`]: Windows.Graphics.Capture screen/window capture (Windows only).
//! * `fake` feature: [`FakeAudio`] (440 Hz tone) and [`FakeScreen`] (color bars).
//!
//! All of them exchange raw media through a shared [`MediaHub`].

mod audio;
#[cfg(feature = "fake")]
mod fake;
mod hub;
mod screen;
mod token;
mod transport;
pub mod video;

pub use audio::{AudioProcessing, CpalAudio, Resampler};
#[cfg(feature = "fake")]
pub use fake::{
    tone_chunk, FakeAudio, FakeScreen, TestPattern, FAKE_INPUT_ID, FAKE_MONITOR_ID, FAKE_OUTPUT_ID,
    FAKE_WINDOW_ID,
};
pub use hub::{MediaHub, MediaStats, PreviewConfig, PreviewFrame, SAMPLES_PER_10MS, SAMPLE_RATE};
#[cfg(windows)]
pub use screen::WgcScreen;
pub use token::HttpTokenProvider;
pub use transport::{
    preset_max_bitrate, LiveKitTransport, TransportOptions, VIEWER_IDENTITY_PREFIX,
};
pub use video::encode_jpeg;
