//! Steam Web API (официальный, документированный).
//! https://partner.steamgames.com/doc/webapi/ISteamUser , https://developer.valvesoftware.com/wiki/Steam_Web_API

use super::error::ProviderError;
use super::{PlayerSummary, Presence, PresenceSource, PresenceState, ProfileSource};
use crate::domain::bans::{BanKind, BanRecord};
use crate::domain::ids::SteamId64;
use crate::infrastructure::http::HttpClient;
use crate::infrastructure::redact::{sanitize_text, sanitize_url};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use std::sync::Arc;

pub const RUST_APP_ID: &str = "252490";
const BASE: &str = "https://api.steampowered.com";

pub type KeyFn = Arc<dyn Fn() -> Option<String> + Send + Sync>;

pub struct SteamProvider {
    http: HttpClient,
    key: KeyFn,
}

impl SteamProvider {
    pub fn new(http: HttpClient, key: KeyFn) -> Self {
        Self { http, key }
    }
    fn key(&self) -> Result<String, ProviderError> {
        (self.key)()
            .ok_or_else(|| ProviderError::NotConfigured { hint: "Добавьте Steam Web API key в разделе «Интеграции»".into() })
    }
    fn url(&self, path: &str, params: &[(&str, &str)]) -> Result<reqwest::Url, ProviderError> {
        let key = self.key()?;
        let mut u = reqwest::Url::parse(&format!("{BASE}{path}")).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })?;
        {
            let mut q = u.query_pairs_mut();
            q.append_pair("key", &key);
            for (k, v) in params {
                q.append_pair(k, v);
            }
        }
        Ok(u)
    }
    pub fn is_configured(&self) -> bool {
        (self.key)().is_some()
    }
    /// Проверка ключа: запрос профиля известного публичного аккаунта из документации Valve.
    /// Список друзей. Ok(None) — список скрыт настройками приватности.
    pub async fn friends(&self, id: &SteamId64) -> Result<Option<Vec<Friend>>, ProviderError> {
        let u = self.url("/ISteamUser/GetFriendList/v1/", &[("steamid", id.as_str()), ("relationship", "friend")])?;
        match self.http.get_json::<serde_json::Value>("steam", u, None).await {
            Ok(v) => parse_friends(&v.to_string()).map(Some),
            Err(ProviderError::Forbidden { status: 401, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn test(&self) -> Result<String, ProviderError> {
        let id = SteamId64::parse("76561197960435530").map_err(|_| ProviderError::InvalidData { detail: "id".into() })?;
        let s = self.summaries(&[id]).await?;
        Ok(format!("Ключ принят, получено профилей: {}", s.len()))
    }
}

// ----------------------------------------------------------- парсинг ответов

#[derive(Deserialize)]
struct Wrap<T> {
    response: T,
}

#[derive(Deserialize)]
struct VanityResp {
    steamid: Option<String>,
    success: i64,
}

pub fn parse_vanity(body: &str) -> Result<SteamId64, ProviderError> {
    let w: Wrap<VanityResp> = serde_json::from_str(body).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })?;
    match (w.response.success, w.response.steamid) {
        (1, Some(id)) => SteamId64::parse(&id).map_err(|e| ProviderError::InvalidData { detail: e.to_string() }),
        (42, _) => Err(ProviderError::NotFound { what: "vanity URL".into() }),
        (code, _) => Err(ProviderError::InvalidData { detail: format!("ResolveVanityURL success={code}") }),
    }
}

#[derive(Deserialize)]
struct SummariesResp {
    players: Vec<RawSummary>,
}

#[derive(Deserialize)]
struct RawSummary {
    steamid: String,
    communityvisibilitystate: Option<i64>,
    personaname: Option<String>,
    profileurl: Option<String>,
    avatarfull: Option<String>,
    personastate: Option<i64>,
    timecreated: Option<i64>,
    loccountrycode: Option<String>,
    gameid: Option<String>,
    gameserverip: Option<String>,
}

/// ip:port, только IPv4 и числовой порт.
pub fn valid_server_addr(s: &str) -> bool {
    let Some((ip, port)) = s.rsplit_once(':') else { return false };
    ip.parse::<std::net::Ipv4Addr>().is_ok() && port.parse::<u16>().is_ok_and(|p| p > 0)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Friend {
    pub steam_id: SteamId64,
    pub friend_since: Option<chrono::DateTime<Utc>>,
}

/// GetFriendList: None — список друзей скрыт (HTTP 401).
pub fn parse_friends(body: &str) -> Result<Vec<Friend>, ProviderError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })?;
    Ok(v["friendslist"]["friends"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| {
                    Some(Friend {
                        steam_id: SteamId64::parse(f["steamid"].as_str()?).ok()?,
                        friend_since: f["friend_since"].as_i64().and_then(|t| Utc.timestamp_opt(t, 0).single()),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn parse_summaries(body: &str) -> Result<Vec<PlayerSummary>, ProviderError> {
    let w: Wrap<SummariesResp> = serde_json::from_str(body).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })?;
    let mut out = Vec::new();
    for p in w.response.players {
        let Ok(steam_id) = SteamId64::parse(&p.steamid) else { continue };
        let public = p.communityvisibilitystate == Some(3);
        let visibility = match p.communityvisibilitystate {
            Some(3) => "public",
            Some(2) => "friends_only",
            Some(1) => "private",
            _ => "unknown",
        };
        let in_rust = if public {
            Some(p.gameid.as_deref() == Some(RUST_APP_ID))
        } else if p.gameid.as_deref() == Some(RUST_APP_ID) {
            Some(true)
        } else {
            None
        };
        out.push(PlayerSummary {
            persona_name: p.personaname.map(|n| sanitize_text(&n, 64)),
            avatar_url: p.avatarfull.and_then(|u| sanitize_url(&u, &["steamstatic.com", "akamaihd.net", "cloudflare.steamstatic.com"])),
            profile_url: p.profileurl.and_then(|u| sanitize_url(&u, &["steamcommunity.com"])).or_else(|| Some(steam_id.profile_url())),
            visibility: visibility.into(),
            created_at: p.timecreated.and_then(|t| Utc.timestamp_opt(t, 0).single()),
            country_code: p.loccountrycode.map(|c| sanitize_text(&c, 4)),
            in_rust,
            persona_state: p.personastate,
            game_server_ip: p.gameserverip.filter(|ip| p.gameid.as_deref() == Some(RUST_APP_ID) && valid_server_addr(ip) && !ip.starts_with("0.0.0.0")),
            steam_id,
        });
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawBans {
    steam_id: String,
    community_banned: bool,
    #[serde(rename = "VACBanned")]
    vac_banned: bool,
    #[serde(rename = "NumberOfVACBans")]
    number_of_vac_bans: i64,
    days_since_last_ban: i64,
    number_of_game_bans: i64,
    economy_ban: String,
}

#[derive(Deserialize)]
struct BansResp {
    players: Vec<RawBans>,
}

pub fn parse_bans(body: &str, observed_at: chrono::DateTime<Utc>) -> Result<Vec<(SteamId64, Vec<BanRecord>)>, ProviderError> {
    let r: BansResp = serde_json::from_str(body).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })?;
    let mut out = Vec::new();
    for b in r.players {
        let Ok(id) = SteamId64::parse(&b.steam_id) else { continue };
        let any_vac_game = b.number_of_vac_bans > 0 || b.number_of_game_bans > 0;
        let days = any_vac_game.then_some(b.days_since_last_ban);
        let src = Some(id.profile_url());
        let mk = |kind, count: Option<i64>, active: bool, reason: Option<String>| BanRecord {
            kind,
            provider: "steam".into(),
            count,
            active,
            days_since_last: if matches!(kind, BanKind::Vac | BanKind::Game) { days } else { None },
            reason,
            project_name: None,
            revoked: None,
            source_url: src.clone(),
            observed_at,
        };
        out.push((
            id.clone(),
            vec![
                mk(BanKind::Vac, Some(b.number_of_vac_bans), b.vac_banned, None),
                mk(BanKind::Game, Some(b.number_of_game_bans), b.number_of_game_bans > 0, None),
                mk(BanKind::Community, None, b.community_banned, None),
                mk(BanKind::Economy, None, b.economy_ban != "none", (b.economy_ban != "none").then(|| sanitize_text(&b.economy_ban, 32))),
            ],
        ));
    }
    Ok(out)
}

#[derive(Deserialize)]
struct OwnedResp {
    games: Option<Vec<OwnedGame>>,
}
#[derive(Deserialize)]
struct OwnedGame {
    appid: i64,
    playtime_forever: i64,
}

/// None — данные об играх скрыты.
pub fn parse_owned_rust_hours(body: &str) -> Result<Option<f64>, ProviderError> {
    let w: Wrap<OwnedResp> = serde_json::from_str(body).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })?;
    Ok(w.response.games.and_then(|g| g.into_iter().find(|x| x.appid == 252_490).map(|x| x.playtime_forever as f64 / 60.0)))
}

async fn get_text(http: &HttpClient, url: reqwest::Url) -> Result<String, ProviderError> {
    let v: serde_json::Value = http.get_json("steam", url, None).await?;
    Ok(v.to_string())
}

#[async_trait]
impl ProfileSource for SteamProvider {
    fn id(&self) -> &'static str {
        "steam"
    }
    async fn resolve_vanity(&self, vanity: &str) -> Result<SteamId64, ProviderError> {
        let u = self.url("/ISteamUser/ResolveVanityURL/v1/", &[("vanityurl", vanity), ("url_type", "1")])?;
        parse_vanity(&get_text(&self.http, u).await?)
    }
    async fn summaries(&self, ids: &[SteamId64]) -> Result<Vec<PlayerSummary>, ProviderError> {
        let mut out = Vec::new();
        for chunk in ids.chunks(100) {
            let joined = chunk.iter().map(|i| i.as_str()).collect::<Vec<_>>().join(",");
            let u = self.url("/ISteamUser/GetPlayerSummaries/v2/", &[("steamids", &joined)])?;
            out.extend(parse_summaries(&get_text(&self.http, u).await?)?);
        }
        Ok(out)
    }
    async fn bans(&self, ids: &[SteamId64]) -> Result<Vec<(SteamId64, Vec<BanRecord>)>, ProviderError> {
        let mut out = Vec::new();
        for chunk in ids.chunks(100) {
            let joined = chunk.iter().map(|i| i.as_str()).collect::<Vec<_>>().join(",");
            let u = self.url("/ISteamUser/GetPlayerBans/v1/", &[("steamids", &joined)])?;
            out.extend(parse_bans(&get_text(&self.http, u).await?, Utc::now())?);
        }
        Ok(out)
    }
    async fn rust_hours(&self, id: &SteamId64) -> Result<Option<f64>, ProviderError> {
        let u =
            self.url("/IPlayerService/GetOwnedGames/v1/", &[("steamid", id.as_str()), ("include_played_free_games", "1"), ("appids_filter[0]", RUST_APP_ID)])?;
        parse_owned_rust_hours(&get_text(&self.http, u).await?)
    }
}

/// Присутствие по данным Steam: «в Rust» / «не в Rust». Сервер неизвестен.
/// Закрытый профиль → Unknown (не офлайн).
pub fn presence_from_summary(s: &PlayerSummary) -> Presence {
    let state = match s.in_rust {
        Some(true) => PresenceState::Online,
        Some(false) => PresenceState::Offline,
        None => PresenceState::Unknown,
    };
    Presence {
        steam_id: s.steam_id.clone(),
        game_server_ip: s.game_server_ip.clone(),
        state,
        server_id: None,
        server_name: None,
        persona_name: s.persona_name.clone(),
    }
}

#[async_trait]
impl PresenceSource for SteamProvider {
    fn id(&self) -> &'static str {
        "steam"
    }
    fn min_poll_secs(&self) -> u64 {
        60
    }
    async fn presence(&self, ids: &[SteamId64]) -> Result<Vec<Presence>, ProviderError> {
        let s = self.summaries(ids).await?;
        let mut out: Vec<Presence> = s.iter().map(presence_from_summary).collect();
        // Отсутствующие в ответе — неизвестно.
        for id in ids {
            if !out.iter().any(|p| &p.steam_id == id) {
                out.push(Presence {
                    steam_id: id.clone(),
                    game_server_ip: None,
                    state: PresenceState::Unknown,
                    server_id: None,
                    server_name: None,
                    persona_name: None,
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fx(name: &str) -> String {
        std::fs::read_to_string(format!("{}/../tests/fixtures/steam/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn contract_summaries() {
        let s = parse_summaries(&fx("player_summaries.json")).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].in_rust, Some(true));
        assert_eq!(s[0].visibility, "public");
        assert!(s[0].created_at.is_some());
        assert_eq!(s[1].visibility, "private");
        assert_eq!(s[1].in_rust, None, "закрытый профиль — неизвестно, не офлайн");
        assert_eq!(presence_from_summary(&s[1]).state, PresenceState::Unknown);
        assert_eq!(s[2].in_rust, Some(false));
        assert_eq!(s[2].avatar_url, None, "javascript: URL отброшен");
        assert_eq!(s[2].profile_url.as_deref(), Some("https://steamcommunity.com/profiles/76561197960287932"), "чужой хост заменён");
    }

    #[test]
    fn contract_friends_and_server_ip() {
        let f = parse_friends(&fx("friends.json")).unwrap();
        assert_eq!(f.len(), 2);
        assert!(f[0].friend_since.is_some());
        let s = parse_summaries(&fx("player_summaries.json")).unwrap();
        assert_eq!(s[0].game_server_ip.as_deref(), Some("203.0.113.10:28015"));
        assert_eq!(s[2].game_server_ip, None, "не Rust — IP не используется");
        assert!(!valid_server_addr("evil:host"));
        assert!(!valid_server_addr("1.2.3.4:0"));
    }

    #[test]
    fn contract_bans_game_ban_not_rust_specific() {
        let b = parse_bans(&fx("player_bans.json"), Utc::now()).unwrap();
        let first = &b[0].1;
        let game = first.iter().find(|x| x.kind == BanKind::Game).unwrap();
        assert_eq!(game.count, Some(1));
        assert!(game.project_name.is_none());
        let second = &b[1].1;
        assert!(second.iter().any(|x| x.kind == BanKind::Vac && x.active && x.count == Some(2)));
        assert!(second.iter().any(|x| x.kind == BanKind::Economy && x.active));
        assert!(second.iter().any(|x| x.kind == BanKind::Community && x.active));
    }

    #[test]
    fn contract_vanity_and_hours() {
        assert_eq!(parse_vanity(&fx("resolve_vanity_ok.json")).unwrap().as_str(), "76561197960287930");
        assert!(matches!(parse_vanity(&fx("resolve_vanity_nomatch.json")), Err(ProviderError::NotFound { .. })));
        let h = parse_owned_rust_hours(&fx("owned_games_rust.json")).unwrap().unwrap();
        assert!((h - 2057.6).abs() < 0.1);
        assert_eq!(parse_owned_rust_hours(&fx("owned_games_private.json")).unwrap(), None);
        assert!(parse_summaries("{not json").is_err());
    }
}
