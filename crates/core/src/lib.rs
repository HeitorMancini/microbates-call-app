//! Sheeshcord room core: a UI- and LiveKit-agnostic state machine for a voice room.
//!
//! [`RoomController`] drives [`Transport`], [`AudioSource`] and [`ScreenSource`]
//! implementations and broadcasts [`RoomEvent`]s matching the `room://*` contract.

pub mod backoff;
pub mod mock;
pub mod room;
pub mod traits;
pub mod types;

pub use backoff::Backoff;
pub use room::{CoreError, CoreResult, Dependencies, RoomConfig, RoomController};
pub use traits::{
    AudioSource, BoxError, ScreenSource, Session, TokenProvider, Transport, TransportEvent,
};
pub use types::*;
