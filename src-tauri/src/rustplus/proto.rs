//! Protobuf-сообщения: протокол Rust+ (подмножество rustplus.proto), Android checkin и MCS (Google push).
//! Номера полей сверены с rustplus.proto (liamcottle/rustplus.js) и mcs.proto/checkin.proto (Chromium).
#![allow(clippy::doc_markdown)]

use prost::Message;

// ------------------------------------------------------------------ Rust+

#[derive(Clone, PartialEq, Message)]
pub struct AppEmpty {}

#[derive(Clone, PartialEq, Message)]
pub struct AppSendMessage {
    #[prost(string, required, tag = "1")]
    pub message: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppRequest {
    #[prost(uint32, required, tag = "1")]
    pub seq: u32,
    #[prost(uint64, required, tag = "2")]
    pub player_id: u64,
    #[prost(int32, required, tag = "3")]
    pub player_token: i32,
    #[prost(uint32, optional, tag = "4")]
    pub entity_id: Option<u32>,
    #[prost(message, optional, tag = "8")]
    pub get_info: Option<AppEmpty>,
    #[prost(message, optional, tag = "9")]
    pub get_time: Option<AppEmpty>,
    #[prost(message, optional, tag = "11")]
    pub get_team_info: Option<AppEmpty>,
    #[prost(message, optional, tag = "13")]
    pub send_team_message: Option<AppSendMessage>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppMessage {
    #[prost(message, optional, tag = "1")]
    pub response: Option<AppResponse>,
    #[prost(message, optional, tag = "2")]
    pub broadcast: Option<AppBroadcast>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppResponse {
    #[prost(uint32, optional, tag = "1")]
    pub seq: Option<u32>,
    #[prost(message, optional, tag = "4")]
    pub success: Option<AppEmpty>,
    #[prost(message, optional, tag = "5")]
    pub error: Option<AppError>,
    #[prost(message, optional, tag = "6")]
    pub info: Option<AppInfo>,
    #[prost(message, optional, tag = "9")]
    pub team_info: Option<AppTeamInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppError {
    #[prost(string, optional, tag = "1")]
    pub error: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppBroadcast {
    #[prost(message, optional, tag = "4")]
    pub team_changed: Option<AppTeamChanged>,
    #[prost(message, optional, tag = "5")]
    pub team_message: Option<AppNewTeamMessage>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppTeamChanged {
    #[prost(uint64, optional, tag = "1")]
    pub player_id: Option<u64>,
    #[prost(message, optional, tag = "2")]
    pub team_info: Option<AppTeamInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppNewTeamMessage {
    #[prost(message, optional, tag = "1")]
    pub message: Option<AppTeamMessage>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppTeamMessage {
    #[prost(uint64, optional, tag = "1")]
    pub steam_id: Option<u64>,
    #[prost(string, optional, tag = "2")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub message: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub color: Option<String>,
    #[prost(uint32, optional, tag = "5")]
    pub time: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppTeamInfo {
    #[prost(uint64, optional, tag = "1")]
    pub leader_steam_id: Option<u64>,
    #[prost(message, repeated, tag = "2")]
    pub members: Vec<Member>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Member {
    #[prost(uint64, optional, tag = "1")]
    pub steam_id: Option<u64>,
    #[prost(string, optional, tag = "2")]
    pub name: Option<String>,
    #[prost(float, optional, tag = "3")]
    pub x: Option<f32>,
    #[prost(float, optional, tag = "4")]
    pub y: Option<f32>,
    #[prost(bool, optional, tag = "5")]
    pub is_online: Option<bool>,
    #[prost(uint32, optional, tag = "6")]
    pub spawn_time: Option<u32>,
    #[prost(bool, optional, tag = "7")]
    pub is_alive: Option<bool>,
    #[prost(uint32, optional, tag = "8")]
    pub death_time: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppInfo {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub map: Option<String>,
    #[prost(uint32, optional, tag = "5")]
    pub map_size: Option<u32>,
    #[prost(uint32, optional, tag = "6")]
    pub wipe_time: Option<u32>,
    #[prost(uint32, optional, tag = "7")]
    pub players: Option<u32>,
    #[prost(uint32, optional, tag = "8")]
    pub max_players: Option<u32>,
}

// ------------------------------------------------------------------ Android checkin

#[derive(Clone, PartialEq, Message)]
pub struct ChromeBuildProto {
    #[prost(int32, optional, tag = "1")]
    pub platform: Option<i32>,
    #[prost(string, optional, tag = "2")]
    pub chrome_version: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub channel: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AndroidCheckinProto {
    #[prost(int32, optional, tag = "12")]
    pub r#type: Option<i32>,
    #[prost(message, optional, tag = "13")]
    pub chrome_build: Option<ChromeBuildProto>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AndroidCheckinRequest {
    #[prost(int64, optional, tag = "2")]
    pub id: Option<i64>,
    #[prost(message, required, tag = "4")]
    pub checkin: AndroidCheckinProto,
    #[prost(fixed64, optional, tag = "13")]
    pub security_token: Option<u64>,
    #[prost(int32, optional, tag = "14")]
    pub version: Option<i32>,
    #[prost(int32, optional, tag = "22")]
    pub user_serial_number: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AndroidCheckinResponse {
    #[prost(bool, optional, tag = "1")]
    pub stats_ok: Option<bool>,
    #[prost(fixed64, optional, tag = "7")]
    pub android_id: Option<u64>,
    #[prost(fixed64, optional, tag = "8")]
    pub security_token: Option<u64>,
}

// ------------------------------------------------------------------ MCS

#[derive(Clone, PartialEq, Message)]
pub struct Setting {
    #[prost(string, required, tag = "1")]
    pub name: String,
    #[prost(string, required, tag = "2")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct LoginRequest {
    #[prost(string, required, tag = "1")]
    pub id: String,
    #[prost(string, required, tag = "2")]
    pub domain: String,
    #[prost(string, required, tag = "3")]
    pub user: String,
    #[prost(string, required, tag = "4")]
    pub resource: String,
    #[prost(string, required, tag = "5")]
    pub auth_token: String,
    #[prost(string, optional, tag = "6")]
    pub device_id: Option<String>,
    #[prost(message, repeated, tag = "8")]
    pub setting: Vec<Setting>,
    #[prost(string, repeated, tag = "10")]
    pub received_persistent_id: Vec<String>,
    #[prost(bool, optional, tag = "12")]
    pub adaptive_heartbeat: Option<bool>,
    #[prost(bool, optional, tag = "14")]
    pub use_rmq2: Option<bool>,
    #[prost(int32, optional, tag = "16")]
    pub auth_service: Option<i32>,
    #[prost(int32, optional, tag = "17")]
    pub network_type: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ErrorInfo {
    #[prost(int32, optional, tag = "1")]
    pub code: Option<i32>,
    #[prost(string, optional, tag = "2")]
    pub message: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct LoginResponse {
    #[prost(string, optional, tag = "1")]
    pub id: Option<String>,
    #[prost(message, optional, tag = "3")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppData {
    #[prost(string, required, tag = "1")]
    pub key: String,
    #[prost(string, required, tag = "2")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct DataMessageStanza {
    #[prost(string, optional, tag = "2")]
    pub id: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub from: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub category: Option<String>,
    #[prost(message, repeated, tag = "7")]
    pub app_data: Vec<AppData>,
    #[prost(string, optional, tag = "9")]
    pub persistent_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct HeartbeatAck {
    #[prost(int32, optional, tag = "1")]
    pub stream_id: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustplus_request_roundtrip() {
        let r = AppRequest { seq: 7, player_id: 76561197960287930, player_token: -123456, get_team_info: Some(AppEmpty {}), ..Default::default() };
        let b = r.encode_to_vec();
        let back = AppRequest::decode(b.as_slice()).unwrap();
        assert_eq!(back, r);
        // поле 11 (getTeamInfo) действительно закодировано
        assert!(b.contains(&((11 << 3) | 2)));
    }

    #[test]
    fn team_info_decodes() {
        let ti = AppMessage {
            response: Some(AppResponse {
                seq: Some(1),
                team_info: Some(AppTeamInfo {
                    leader_steam_id: Some(1),
                    members: vec![Member {
                        steam_id: Some(5),
                        name: Some("Mate".into()),
                        x: Some(10.0),
                        y: Some(20.0),
                        is_online: Some(true),
                        spawn_time: Some(1),
                        is_alive: Some(false),
                        death_time: Some(99),
                    }],
                }),
                ..Default::default()
            }),
            broadcast: None,
        };
        let back = AppMessage::decode(ti.encode_to_vec().as_slice()).unwrap();
        assert_eq!(back.response.unwrap().team_info.unwrap().members[0].death_time, Some(99));
    }
}
