use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RoomStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

/// Payload of the `room://state` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoomState {
    pub status: RoomStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RoomState {
    pub fn new(status: RoomStatus) -> Self {
        Self {
            status,
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: RoomStatus::Error,
            error: Some(message.into()),
        }
    }
}

/// Item of the `room://participants` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: String,
    pub name: String,
    pub muted: bool,
    pub speaking: bool,
    pub sharing_screen: bool,
    pub is_local: bool,
}

/// A participant as reported by the transport (never the local one).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemoteParticipant {
    pub id: String,
    pub name: String,
    pub muted: bool,
    pub sharing_screen: bool,
}

impl From<RemoteParticipant> for Participant {
    fn from(p: RemoteParticipant) -> Self {
        Self {
            id: p.id,
            name: p.name,
            muted: p.muted,
            speaking: false,
            sharing_screen: p.sharing_screen,
            is_local: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AudioDevices {
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenSourceKind {
    Monitor,
    Window,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSourceInfo {
    pub id: String,
    pub name: String,
    pub kind: ScreenSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScreenPreset {
    #[serde(rename = "1080p30")]
    P1080Fps30,
    #[serde(rename = "1080p60")]
    P1080Fps60,
    #[serde(rename = "720p30")]
    P720Fps30,
}

impl ScreenPreset {
    pub fn width(self) -> u32 {
        match self {
            Self::P1080Fps30 | Self::P1080Fps60 => 1920,
            Self::P720Fps30 => 1280,
        }
    }

    pub fn height(self) -> u32 {
        match self {
            Self::P1080Fps30 | Self::P1080Fps60 => 1080,
            Self::P720Fps30 => 720,
        }
    }

    pub fn fps(self) -> u32 {
        match self {
            Self::P1080Fps60 => 60,
            Self::P1080Fps30 | Self::P720Fps30 => 30,
        }
    }
}

/// Arguments of `join_room`. `server_url` is the token-server base URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinParams {
    pub server_url: String,
    pub room: String,
    pub name: String,
}

/// Response of the token server (`POST /token`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomEvent {
    State(RoomState),
    Participants(Vec<Participant>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub state: RoomState,
    pub participants: Vec<Participant>,
    pub muted: bool,
    pub deafened: bool,
    pub sharing: Option<ScreenPreset>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn room_state_matches_contract() {
        let v = serde_json::to_value(RoomState::new(RoomStatus::Reconnecting)).unwrap();
        assert_eq!(v, json!({ "status": "reconnecting" }));
        let v = serde_json::to_value(RoomState::error("boom")).unwrap();
        assert_eq!(v, json!({ "status": "error", "error": "boom" }));
    }

    #[test]
    fn participant_uses_camel_case() {
        let p = Participant {
            id: "a".into(),
            name: "A".into(),
            muted: true,
            speaking: false,
            sharing_screen: true,
            is_local: false,
        };
        assert_eq!(
            serde_json::to_value(p).unwrap(),
            json!({ "id": "a", "name": "A", "muted": true, "speaking": false, "sharingScreen": true, "isLocal": false })
        );
    }

    #[test]
    fn presets_use_contract_names() {
        let p: ScreenPreset = serde_json::from_value(json!("1080p60")).unwrap();
        assert_eq!(p, ScreenPreset::P1080Fps60);
        assert_eq!(p.fps(), 60);
        assert_eq!(
            serde_json::to_value(ScreenPreset::P720Fps30).unwrap(),
            json!("720p30")
        );
    }
}
