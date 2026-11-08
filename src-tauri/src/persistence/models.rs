//! Строки БД, сериализуемые во frontend (snake_case JSON). SteamID64 — всегда строка.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PlayerRow {
    pub steam_id: String,
    pub persona_name: Option<String>,
    pub avatar_url: Option<String>,
    pub profile_url: Option<String>,
    pub visibility: String,
    pub account_created_at: Option<String>,
    pub country_code: Option<String>,
    pub rust_hours: Option<f64>,
    pub rust_hours_status: String,
    pub steam_in_game: Option<bool>,
    pub steam_status: String,
    pub steam_fetched_at: Option<String>,
    pub is_demo: bool,
    pub favorite: bool,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AliasRow {
    pub name: String,
    pub source: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BanRow {
    pub steam_id: String,
    pub provider: String,
    pub kind: String,
    pub project_name: String,
    pub count: Option<i64>,
    pub active: bool,
    pub days_since_last: Option<i64>,
    pub reason: Option<String>,
    pub revoked: Option<bool>,
    pub source_url: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProviderLinkRow {
    pub steam_id: String,
    pub provider: String,
    pub external_id: String,
    pub match_evidence: String,
    pub confidence: String,
    pub source_url: Option<String>,
    pub linked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerRow {
    pub id: String,
    pub provider: String,
    pub external_id: String,
    pub name: String,
    pub address: Option<String>,
    pub country: Option<String>,
    pub map: Option<String>,
    pub players: Option<i64>,
    pub max_players: Option<i64>,
    pub queue: Option<i64>,
    pub online: Option<bool>,
    pub trust: Option<String>,
    pub last_wipe_at: Option<String>,
    pub next_wipe_at: Option<String>,
    pub followed: bool,
    pub source_url: Option<String>,
    pub fetched_at: String,
    pub is_demo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PopulationPoint {
    pub t: String,
    pub players: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionRow {
    pub id: i64,
    pub steam_id: String,
    pub provider: String,
    pub kind: String,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub start_lower: String,
    pub start_upper: String,
    pub end_lower: String,
    pub end_upper: Option<String>,
    pub is_open: bool,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CoverageRow {
    pub provider: String,
    pub start_at: String,
    pub end_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObservationRow {
    pub provider: String,
    pub observed_at: String,
    pub state: String,
    pub server_id: Option<String>,
    pub poll_interval_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TrackedRow {
    pub steam_id: String,
    pub enabled: bool,
    pub notify: bool,
    pub added_at: String,
    pub last_state: Option<String>,
    pub last_server: Option<String>,
    pub last_polled_at: Option<String>,
    pub last_success_at: Option<String>,
    pub persona_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_demo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventRow {
    pub id: i64,
    pub steam_id: Option<String>,
    pub persona_name: Option<String>,
    pub provider: String,
    pub kind: String,
    pub occurred_lower: String,
    pub occurred_upper: String,
    pub approximate: bool,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub details: String,
    pub created_at: String,
    pub is_read: bool,
    pub notified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SquadRow {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub member_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SquadMemberRow {
    pub steam_id: String,
    pub persona_name: Option<String>,
    pub avatar_url: Option<String>,
    pub provenance: String,
    pub added_at: String,
    pub is_demo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HealthRow {
    pub provider: String,
    pub status: String,
    pub last_success_at: Option<String>,
    pub last_error_at: Option<String>,
    pub last_error_kind: Option<String>,
    pub last_error: Option<String>,
    pub consecutive_failures: i64,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SearchHistoryRow {
    pub query: String,
    pub steam_id: Option<String>,
    pub searched_at: String,
}
