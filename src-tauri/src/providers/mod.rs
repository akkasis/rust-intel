//! Провайдеры данных: явные возможности (capabilities) и типизированные ошибки.

pub mod battlemetrics;
pub mod bmru;
pub mod demo;
pub mod error;
pub mod steam;

use crate::domain::bans::BanRecord;
use crate::domain::ids::SteamId64;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use error::ProviderError;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    LookupPlayer,
    ResolveVanity,
    SearchByName,
    GetBans,
    OwnedGameHours,
    PlayerSessions,
    CurrentPresence,
    ServerCatalog,
    ServerPopulation,
    Wipes,
    CombatStats,
    ProjectBans,
    Notifications,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessStatus {
    /// Проверено реальным запросом в этом приложении.
    Verified,
    /// Документировано, но не проверено (нет ключа/подписки).
    NotTested,
    /// Источник ответил отказом (например, требуется подписка).
    Denied,
    /// Не поддерживается источником.
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilitySupport {
    pub capability: Capability,
    pub supported: bool,
    pub requires: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub docs_url: &'static str,
    pub credential: Option<&'static str>,
    pub capabilities: Vec<CapabilitySupport>,
    pub terms_note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerSummary {
    pub steam_id: SteamId64,
    pub persona_name: Option<String>,
    pub avatar_url: Option<String>,
    pub profile_url: Option<String>,
    /// public | private | friends_only
    pub visibility: String,
    pub created_at: Option<DateTime<Utc>>,
    pub country_code: Option<String>,
    /// Some(true) — Steam сообщает, что запущен Rust; None — неизвестно (закрытый профиль).
    pub in_rust: Option<bool>,
    pub persona_state: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Online,
    Offline,
    Unknown,
}

impl PresenceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Presence {
    pub steam_id: SteamId64,
    pub state: PresenceState,
    /// Ключ сервера в БД (`provider:external_id`), если источник знает сервер.
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub persona_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSession {
    pub external_id: String,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub start: DateTime<Utc>,
    pub stop: Option<DateTime<Utc>>,
    pub player_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub provider: &'static str,
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
    pub last_wipe_at: Option<DateTime<Utc>>,
    pub next_wipe_at: Option<DateTime<Utc>>,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NameCandidate {
    pub provider: &'static str,
    pub external_id: String,
    pub name: String,
    /// SteamID только если провайдер вернул идентификатор явно.
    pub steam_id: Option<SteamId64>,
    pub last_seen: Option<DateTime<Utc>>,
    pub source_url: Option<String>,
}

/// Steam-подобный источник профиля и банов.
#[async_trait]
pub trait ProfileSource: Send + Sync {
    fn id(&self) -> &'static str;
    async fn resolve_vanity(&self, vanity: &str) -> Result<SteamId64, ProviderError>;
    async fn summaries(&self, ids: &[SteamId64]) -> Result<Vec<PlayerSummary>, ProviderError>;
    async fn bans(&self, ids: &[SteamId64]) -> Result<Vec<(SteamId64, Vec<BanRecord>)>, ProviderError>;
    /// Ok(None) — часы скрыты настройками приватности.
    async fn rust_hours(&self, id: &SteamId64) -> Result<Option<f64>, ProviderError>;
}

/// Источник текущего присутствия (для трекера).
#[async_trait]
pub trait PresenceSource: Send + Sync {
    fn id(&self) -> &'static str;
    /// Минимально допустимый интервал опроса.
    fn min_poll_secs(&self) -> u64;
    async fn presence(&self, ids: &[SteamId64]) -> Result<Vec<Presence>, ProviderError>;
}

/// Каталог серверов и серверная статистика (не история игроков!).
#[async_trait]
pub trait ServerCatalog: Send + Sync {
    fn id(&self) -> &'static str;
    async fn search_servers(&self, query: Option<&str>) -> Result<Vec<ServerInfo>, ProviderError>;
    async fn population(&self, external_id: &str, period: &str) -> Result<Vec<(DateTime<Utc>, f64)>, ProviderError>;
}

pub fn descriptors() -> Vec<ProviderDescriptor> {
    use Capability::*;
    let c = |capability, supported, requires, note| CapabilitySupport { capability, supported, requires, note };
    vec![
        ProviderDescriptor {
            id: "steam",
            name: "Steam Web API",
            kind: "official",
            docs_url: "https://partner.steamgames.com/doc/webapi/ISteamUser",
            credential: Some("steam_api_key"),
            capabilities: vec![
                c(LookupPlayer, true, "Steam Web API key", "ISteamUser/GetPlayerSummaries/v2"),
                c(ResolveVanity, true, "Steam Web API key", "ISteamUser/ResolveVanityURL/v1"),
                c(GetBans, true, "Steam Web API key", "ISteamUser/GetPlayerBans/v1 — VAC/Game/Community/Economy; Game Ban не указывает игру"),
                c(OwnedGameHours, true, "Steam Web API key", "IPlayerService/GetOwnedGames/v1 appid 252490 — только при открытых игровых данных"),
                c(CurrentPresence, true, "Steam Web API key", "gameid=252490 в GetPlayerSummaries — «в Rust по данным Steam», без сервера"),
                c(SearchByName, false, "", "Steam Web API не ищет по нику"),
                c(PlayerSessions, false, "", "История сессий не предоставляется"),
            ],
            terms_note: "Steam Web API Terms of Use; лимит 100 000 вызовов в сутки на ключ.",
        },
        ProviderDescriptor {
            id: "battlemetrics",
            name: "BattleMetrics.com API",
            kind: "official_paid",
            docs_url: "https://www.battlemetrics.com/developers/documentation",
            credential: Some("battlemetrics_token"),
            capabilities: vec![
                c(SearchByName, true, "Токен + подписка", "GET /players?filter[search]= — кандидаты без автоматической привязки к SteamID"),
                c(LookupPlayer, true, "Токен + подписка; сопоставление SteamID — POST /players/match", "Привязка только по точному идентификатору steamID"),
                c(PlayerSessions, true, "Токен + подписка", "GET /players/{id}/relationships/sessions"),
                c(CurrentPresence, true, "Токен + подписка", "Открытая сессия (stop = null) на сервере, который отслеживает BattleMetrics"),
                c(ServerCatalog, true, "Токен + подписка", "GET /servers?filter[game]=rust"),
                c(ServerPopulation, true, "Токен + подписка", "GET /servers/{id}/player-count-history"),
                c(CombatStats, false, "", "Нет авторитетных полей боевой статистики"),
            ],
            terms_note: "В ноябре 2026 API без подписки отвечает 403 «A subscription is required». Не обходить.",
        },
        ProviderDescriptor {
            id: "battlemetrics_ru",
            name: "battlemetrics.ru API v1",
            kind: "official_free_key",
            docs_url: "https://battlemetrics.ru/api",
            credential: Some("battlemetrics_ru_key"),
            capabilities: vec![
                c(ServerCatalog, true, "Бесплатный ключ bm_live_…", "GET /api/v1/servers?game=rust"),
                c(ServerPopulation, true, "Бесплатный ключ", "GET /api/v1/servers/{id}/history?period=24h|7d|30d — средний онлайн сервера"),
                c(Wipes, true, "Бесплатный ключ", "GET /api/v1/wipes?game=rust (fact=true — наблюдаемый вайп)"),
                c(PlayerSessions, false, "", "Только серверный уровень: нет поиска игроков и истории игроков"),
            ],
            terms_note: "60 запросов/мин на ключ; данные обновляются раз в минуту. Не путать с battlemetrics.com.",
        },
        ProviderDescriptor {
            id: "project_bans",
            name: "Баны проектов",
            kind: "adapter_slot",
            docs_url: "",
            credential: None,
            capabilities: vec![c(
                ProjectBans,
                false,
                "Документированный публичный API конкретного проекта",
                "Адаптеры не включены: нет подтверждённых документированных API. Произвольные URL не принимаются (защита от SSRF).",
            )],
            terms_note: "Подключать только законные и документированные источники.",
        },
        ProviderDescriptor {
            id: "telegram",
            name: "Telegram Bot API",
            kind: "notification",
            docs_url: "https://core.telegram.org/bots/api",
            credential: Some("telegram_bot_token"),
            capabilities: vec![c(Notifications, true, "Токен бота + chat ID", "sendMessage — только видимая игровая активность, без приватных данных")],
            terms_note: "Сообщения отправляются с этого ПК, пока приложение запущено.",
        },
    ]
}
