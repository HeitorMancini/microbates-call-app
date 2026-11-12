//! Tipos trafegados na fronteira IPC (ver CONTRACT.md). Serializados em camelCase.

use serde::{Deserialize, Serialize};

pub const EVENT_STATE: &str = "room://state";
pub const EVENT_PARTICIPANTS: &str = "room://participants";
pub const EVENT_SCREEN_FRAME: &str = "room://screen-frame";

// Os tipos compartilhados vêm de `sheeshcord_core` (mesma serialização do contrato).
pub use sheeshcord_core::{
    AudioDevices, Device, DeviceKind, Participant, RoomState, RoomStatus as ConnectionStatus,
    ScreenPreset, ScreenSourceInfo as ScreenSource, ScreenSourceKind,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenFrame {
    pub participant_id: String,
    pub width: u32,
    pub height: u32,
    pub jpeg_base64: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn participant_serializes_in_camel_case() {
        let p = Participant {
            id: "a".into(),
            name: "Ana".into(),
            muted: false,
            speaking: true,
            sharing_screen: true,
            is_local: true,
        };
        assert_eq!(
            serde_json::to_value(p).unwrap(),
            json!({"id":"a","name":"Ana","muted":false,"speaking":true,"sharingScreen":true,"isLocal":true})
        );
    }

    #[test]
    fn state_omits_missing_error() {
        let v = serde_json::to_value(RoomState::new(ConnectionStatus::Connecting)).unwrap();
        assert_eq!(v, json!({"status":"connecting"}));
    }

    #[test]
    fn preset_and_kind_use_contract_strings() {
        let p: ScreenPreset = serde_json::from_value(json!("1080p60")).unwrap();
        assert_eq!(p, ScreenPreset::P1080Fps60);
        let k: DeviceKind = serde_json::from_value(json!("output")).unwrap();
        assert_eq!(k, DeviceKind::Output);
        assert!(serde_json::from_value::<ScreenPreset>(json!("4k")).is_err());
    }
}
