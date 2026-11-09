//! События трекера.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    FirstSeen,
    JoinedObservedServer,
    LeftObservedServer,
    SwitchedObservedServer,
    /// Steam сообщает, что игрок запустил Rust (gameid=252490)
    SteamInGameStarted,
    SteamInGameStopped,
    NameChanged,
    BanChange,
    SourceUnavailable,
    SourceRestored,
    /// Несколько участников клана BM зашли почти одновременно.
    ClanGathering,
    /// Тиммейт погиб (Rust+).
    TeammateDied,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FirstSeen => "first_seen",
            Self::JoinedObservedServer => "joined_observed_server",
            Self::LeftObservedServer => "left_observed_server",
            Self::SwitchedObservedServer => "switched_observed_server",
            Self::SteamInGameStarted => "steam_in_game_started",
            Self::SteamInGameStopped => "steam_in_game_stopped",
            Self::NameChanged => "name_changed",
            Self::BanChange => "ban_change",
            Self::SourceUnavailable => "source_unavailable",
            Self::SourceRestored => "source_restored",
            Self::ClanGathering => "clan_gathering",
            Self::TeammateDied => "teammate_died",
        }
    }
    pub const ALL: [EventKind; 12] = [
        Self::FirstSeen,
        Self::JoinedObservedServer,
        Self::LeftObservedServer,
        Self::SwitchedObservedServer,
        Self::SteamInGameStarted,
        Self::SteamInGameStopped,
        Self::NameChanged,
        Self::BanChange,
        Self::SourceUnavailable,
        Self::SourceRestored,
        Self::ClanGathering,
        Self::TeammateDied,
    ];
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
    pub fn title_ru(&self) -> &'static str {
        match self {
            Self::FirstSeen => "Впервые замечен",
            Self::JoinedObservedServer => "Зашёл на наблюдаемый сервер",
            Self::LeftObservedServer => "Покинул наблюдаемый сервер",
            Self::SwitchedObservedServer => "Сменил сервер",
            Self::SteamInGameStarted => "Запустил Rust (Steam)",
            Self::SteamInGameStopped => "Вышел из Rust (Steam)",
            Self::NameChanged => "Сменил ник",
            Self::BanChange => "Изменение банов",
            Self::SourceUnavailable => "Источник недоступен",
            Self::SourceRestored => "Источник восстановлен",
            Self::ClanGathering => "Клан заходит",
            Self::TeammateDied => "Тиммейт погиб",
        }
    }
}
