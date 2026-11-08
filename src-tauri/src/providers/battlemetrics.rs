//! BattleMetrics.com (официальный JSON:API). Требует токен и, по состоянию на ноябрь 2026,
//! платную подписку — без неё API отвечает 403. Адаптер НЕ имитирует успех.
//!
//! Сопоставление SteamID → BattleMetrics player id выполняется только по точному
//! идентификатору `steamID` из ответа `POST /players/match`. Поиск по нику возвращает кандидатов
//! без привязки к SteamID.

use super::error::ProviderError;
use super::{NameCandidate, Presence, PresenceSource, PresenceState, ProviderSession, ServerCatalog, ServerInfo};
use crate::domain::ids::{BattleMetricsPlayerId, SteamId64};
use crate::infrastructure::http::HttpClient;
use crate::infrastructure::redact::sanitize_text;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

const BASE: &str = "https://api.battlemetrics.com";
pub type KeyFn = Arc<dyn Fn() -> Option<String> + Send + Sync>;

pub struct BattleMetricsProvider {
    http: HttpClient,
    token: KeyFn,
}

fn ts(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&Utc))
}

pub fn server_key(id: &str) -> String {
    format!("battlemetrics:{id}")
}

/// Разбор `POST /players/match`: возвращает player id только для точного совпадения steamID.
pub fn parse_match(body: &Value, steam_id: &SteamId64) -> Option<BattleMetricsPlayerId> {
    body["data"].as_array()?.iter().find_map(|d| {
        let a = &d["attributes"];
        let type_ok = a["type"].is_null() || a["type"] == "steamID";
        (type_ok && a["identifier"].as_str() == Some(steam_id.as_str()))
            .then(|| d["relationships"]["player"]["data"]["id"].as_str().map(|s| BattleMetricsPlayerId(s.to_string())))
            .flatten()
    })
}

/// ID игрока BattleMetrics из ввода: число или ссылка `battlemetrics.com/players/123456`.
pub fn parse_player_id(input: &str) -> Option<BattleMetricsPlayerId> {
    let t = input.trim().trim_end_matches('/');
    if !t.is_empty() && t.len() <= 15 && t.bytes().all(|b| b.is_ascii_digit()) {
        return Some(BattleMetricsPlayerId(t.to_string()));
    }
    let url = url::Url::parse(if t.starts_with("http") { t.to_string() } else { format!("https://{t}") }.as_str()).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "www.battlemetrics.com" && host != "battlemetrics.com" {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.filter(|x| !x.is_empty()).collect();
    match segs.as_slice() {
        ["players", id, ..] | ["rcon", "players", id, ..] if id.len() <= 15 && id.bytes().all(|b| b.is_ascii_digit()) => {
            Some(BattleMetricsPlayerId((*id).to_string()))
        }
        _ => None,
    }
}

/// Статистика игрока на сервере из `GET /players/{id}?include=server` (included[].meta).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BmServerStat {
    pub server_id: String,
    pub server_name: String,
    pub address: Option<String>,
    pub online: bool,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub time_played_secs: Option<i64>,
    pub players: Option<i64>,
    pub max_players: Option<i64>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BmPlayerInfo {
    pub id: BattleMetricsPlayerId,
    pub name: Option<String>,
    /// SteamID, только если токен имеет доступ к идентификаторам (RCON) — иначе None.
    pub steam_id: Option<String>,
    pub names: Vec<String>,
    pub servers: Vec<BmServerStat>,
}

pub fn parse_player_info(body: &Value) -> Option<BmPlayerInfo> {
    let d = &body["data"];
    let id = d["id"].as_str()?.to_string();
    let mut steam_id = None;
    let mut names = Vec::new();
    let mut servers = Vec::new();
    for i in body["included"].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
        match i["type"].as_str() {
            Some("identifier") => match i["attributes"]["type"].as_str() {
                Some("steamID") => steam_id = i["attributes"]["identifier"].as_str().map(str::to_string),
                Some("name") => {
                    if let Some(n) = i["attributes"]["identifier"].as_str() {
                        names.push(sanitize_text(n, 64));
                    }
                }
                _ => {}
            },
            Some("server") => {
                let Some(sid) = i["id"].as_str() else { continue };
                let a = &i["attributes"];
                let m = &i["meta"];
                servers.push(BmServerStat {
                    server_id: server_key(sid),
                    server_name: sanitize_text(a["name"].as_str().unwrap_or("?"), 120),
                    address: match (a["ip"].as_str(), a["port"].as_i64()) {
                        (Some(ip), Some(p)) => Some(format!("{ip}:{p}")),
                        _ => None,
                    },
                    online: m["online"].as_bool().unwrap_or(false),
                    first_seen: ts(&m["firstSeen"]),
                    last_seen: ts(&m["lastSeen"]),
                    time_played_secs: m["timePlayed"].as_i64(),
                    players: a["players"].as_i64(),
                    max_players: a["maxPlayers"].as_i64(),
                    country: a["country"].as_str().map(|c| sanitize_text(c, 4)),
                });
            }
            _ => {}
        }
    }
    servers.sort_by_key(|s| std::cmp::Reverse(s.last_seen));
    Some(BmPlayerInfo { id: BattleMetricsPlayerId(id), name: d["attributes"]["name"].as_str().map(|n| sanitize_text(n, 64)), steam_id, names, servers })
}

/// Онлайн-игроки сервера из `GET /servers/{id}?include=player` (только BM id и ник; SteamID не раскрывается).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerRoster {
    pub server: ServerInfo,
    pub players: Vec<(String, String)>,
}

pub fn parse_server_roster(body: &Value) -> Option<ServerRoster> {
    let server = parse_servers(&serde_json::json!({ "data": [body["data"].clone()] })).into_iter().next()?;
    let players = body["included"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|i| i["type"] == "player")
                .filter_map(|i| {
                    let id = i["id"].as_str()?;
                    id.bytes().all(|b| b.is_ascii_digit()).then(|| (id.to_string(), sanitize_text(i["attributes"]["name"].as_str().unwrap_or("?"), 64)))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ServerRoster { server, players })
}

pub fn parse_sessions(body: &Value) -> Vec<ProviderSession> {
    let mut names: HashMap<String, String> = HashMap::new();
    if let Some(inc) = body["included"].as_array() {
        for i in inc.iter().filter(|i| i["type"] == "server") {
            if let (Some(id), Some(n)) = (i["id"].as_str(), i["attributes"]["name"].as_str()) {
                names.insert(id.to_string(), sanitize_text(n, 120));
            }
        }
    }
    body["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|d| d["type"] == "session")
                .filter_map(|d| {
                    let start = ts(&d["attributes"]["start"])?;
                    let sid = d["relationships"]["server"]["data"]["id"].as_str().map(str::to_string);
                    Some(ProviderSession {
                        external_id: d["id"].as_str()?.to_string(),
                        server_name: sid.as_ref().and_then(|s| names.get(s).cloned()),
                        server_id: sid.map(|s| server_key(&s)),
                        start,
                        stop: ts(&d["attributes"]["stop"]),
                        player_name: d["attributes"]["name"].as_str().map(|n| sanitize_text(n, 64)),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_player_search(body: &Value) -> Vec<NameCandidate> {
    body["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|d| d["type"] == "player")
                .filter_map(|d| {
                    let id = d["id"].as_str()?.to_string();
                    Some(NameCandidate {
                        provider: "battlemetrics",
                        source_url: Some(format!("https://www.battlemetrics.com/players/{id}")),
                        external_id: id,
                        name: sanitize_text(d["attributes"]["name"].as_str().unwrap_or("?"), 64),
                        steam_id: None,
                        last_seen: ts(&d["attributes"]["updatedAt"]),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_servers(body: &Value) -> Vec<ServerInfo> {
    body["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| {
                    let id = d["id"].as_str()?.to_string();
                    let at = &d["attributes"];
                    Some(ServerInfo {
                        provider: "battlemetrics",
                        name: sanitize_text(at["name"].as_str().unwrap_or("?"), 120),
                        address: match (at["ip"].as_str(), at["port"].as_i64()) {
                            (Some(ip), Some(p)) => Some(format!("{ip}:{p}")),
                            _ => None,
                        },
                        country: at["country"].as_str().map(|s| sanitize_text(s, 4)),
                        map: at["details"]["map"].as_str().map(|s| sanitize_text(s, 64)),
                        players: at["players"].as_i64(),
                        max_players: at["maxPlayers"].as_i64(),
                        queue: at["details"]["rust_queued_players"].as_i64(),
                        online: at["status"].as_str().map(|s| s == "online"),
                        trust: None,
                        last_wipe_at: ts(&at["details"]["rust_last_wipe"]),
                        next_wipe_at: None,
                        source_url: Some(format!("https://www.battlemetrics.com/servers/rust/{id}")),
                        external_id: id,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl BattleMetricsProvider {
    pub fn new(http: HttpClient, token: KeyFn) -> Self {
        Self { http, token }
    }
    pub fn is_configured(&self) -> bool {
        (self.token)().is_some()
    }
    fn token(&self) -> Result<String, ProviderError> {
        (self.token)()
            .ok_or_else(|| ProviderError::NotConfigured { hint: "Добавьте токен BattleMetrics.com (нужна подписка)".into() })
    }
    fn url(path: &str, params: &[(&str, &str)]) -> Result<reqwest::Url, ProviderError> {
        reqwest::Url::parse_with_params(&format!("{BASE}{path}"), params).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })
    }
    pub async fn test(&self) -> Result<String, ProviderError> {
        let t = self.token()?;
        let _: Value = self.http.get_json("battlemetrics", Self::url("/servers", &[("filter[game]", "rust"), ("page[size]", "1")])?, Some(&t)).await?;
        Ok("Токен принят".into())
    }
    pub async fn match_steam_id(&self, steam_id: &SteamId64) -> Result<Option<BattleMetricsPlayerId>, ProviderError> {
        let t = self.token()?;
        let body = json!({"data": [{"type": "identifier", "attributes": {"type": "steamID", "identifier": steam_id.as_str()}}]});
        // quick-match доступен обычным токенам; match — организациям с RCON.
        let quick = self.http.post_json::<Value>("battlemetrics", Self::url("/players/quick-match", &[("page[size]", "5")])?, Some(&t), &body).await;
        if let Ok(v) = &quick
            && let Some(id) = parse_match(v, steam_id)
        {
            return Ok(Some(id));
        }
        match self.http.post_json::<Value>("battlemetrics", Self::url("/players/match", &[])?, Some(&t), &body).await {
            Ok(v) => Ok(parse_match(&v, steam_id)),
            Err(e) => match quick {
                Ok(_) => Ok(None),
                Err(q) => Err(if matches!(e, ProviderError::Forbidden { .. }) { q } else { e }),
            },
        }
    }
    pub async fn search_by_name(&self, name: &str) -> Result<Vec<NameCandidate>, ProviderError> {
        let t = self.token()?;
        let v: Value = self.http.get_json("battlemetrics", Self::url("/players", &[("filter[search]", name), ("page[size]", "15")])?, Some(&t)).await?;
        Ok(parse_player_search(&v))
    }
    /// Серверы Rust по IP (поиск BM ищет и по адресу). Возвращает точное совпадение ip:port, иначе все с тем же IP.
    pub async fn servers_by_addr(&self, addr: &str) -> Result<Vec<ServerInfo>, ProviderError> {
        let t = self.token()?;
        let ip = addr.rsplit_once(':').map(|x| x.0).unwrap_or(addr);
        if ip.parse::<std::net::Ipv4Addr>().is_err() {
            return Err(ProviderError::InvalidData { detail: "некорректный IP".into() });
        }
        let v: Value = self
            .http
            .get_json("battlemetrics", Self::url("/servers", &[("filter[search]", ip), ("filter[game]", "rust"), ("page[size]", "20")])?, Some(&t))
            .await?;
        let all: Vec<ServerInfo> = parse_servers(&v).into_iter().filter(|s| s.address.as_deref().is_some_and(|a| a.starts_with(&format!("{ip}:")))).collect();
        let exact: Vec<ServerInfo> = all.iter().filter(|s| s.address.as_deref() == Some(addr)).cloned().collect();
        Ok(if exact.is_empty() { all } else { exact })
    }

    pub async fn server_roster(&self, external_id: &str) -> Result<ServerRoster, ProviderError> {
        let t = self.token()?;
        if external_id.is_empty() || !external_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ProviderError::InvalidData { detail: "некорректный server id".into() });
        }
        let v: Value = self.http.get_json("battlemetrics", Self::url(&format!("/servers/{external_id}"), &[("include", "player")])?, Some(&t)).await?;
        parse_server_roster(&v).ok_or_else(|| ProviderError::InvalidData { detail: "пустой ответ /servers/{id}".into() })
    }

    pub async fn player_info(&self, player: &BattleMetricsPlayerId) -> Result<BmPlayerInfo, ProviderError> {
        let t = self.token()?;
        if !player.0.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ProviderError::InvalidData { detail: "некорректный player id".into() });
        }
        let v: Value =
            self.http.get_json("battlemetrics", Self::url(&format!("/players/{}", player.0), &[("include", "server,identifier")])?, Some(&t)).await?;
        parse_player_info(&v).ok_or_else(|| ProviderError::InvalidData { detail: "пустой ответ /players/{id}".into() })
    }

    pub async fn sessions(&self, player: &BattleMetricsPlayerId) -> Result<Vec<ProviderSession>, ProviderError> {
        let t = self.token()?;
        let path = format!("/players/{}/relationships/sessions", player.0);
        if !player.0.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ProviderError::InvalidData { detail: "некорректный player id".into() });
        }
        let v: Value = self.http.get_json("battlemetrics", Self::url(&path, &[("include", "server"), ("page[size]", "100")])?, Some(&t)).await?;
        Ok(parse_sessions(&v))
    }
}

/// Присутствие: открытая сессия у привязанного игрока. Требует заранее сохранённой привязки.
pub struct BmPresence {
    pub provider: Arc<BattleMetricsProvider>,
    pub links: HashMap<SteamId64, BattleMetricsPlayerId>,
}

#[async_trait]
impl PresenceSource for BmPresence {
    fn id(&self) -> &'static str {
        "battlemetrics"
    }
    fn min_poll_secs(&self) -> u64 {
        120
    }
    async fn presence(&self, ids: &[SteamId64]) -> Result<Vec<Presence>, ProviderError> {
        let mut out = Vec::new();
        for id in ids {
            let Some(bm) = self.links.get(id) else {
                out.push(Presence {
                    steam_id: id.clone(),
                    game_server_ip: None,
                    state: PresenceState::Unknown,
                    server_id: None,
                    server_name: None,
                    persona_name: None,
                });
                continue;
            };
            let info = self.provider.player_info(bm).await?;
            let on = info.servers.iter().find(|x| x.online);
            out.push(Presence {
                steam_id: id.clone(),
                game_server_ip: None,
                state: if on.is_some() { PresenceState::Online } else { PresenceState::Offline },
                server_id: on.map(|o| o.server_id.clone()),
                server_name: on.map(|o| o.server_name.clone()),
                persona_name: None,
            });
        }
        Ok(out)
    }
}

#[async_trait]
impl ServerCatalog for BattleMetricsProvider {
    fn id(&self) -> &'static str {
        "battlemetrics"
    }
    async fn search_servers(&self, query: Option<&str>) -> Result<Vec<ServerInfo>, ProviderError> {
        let t = self.token()?;
        let mut params = vec![("filter[game]", "rust"), ("page[size]", "50"), ("sort", "rank")];
        if let Some(q) = query.filter(|q| !q.is_empty()) {
            params.push(("filter[search]", q));
        }
        let v: Value = self.http.get_json("battlemetrics", Self::url("/servers", &params)?, Some(&t)).await?;
        Ok(parse_servers(&v))
    }
    async fn population(&self, external_id: &str, period: &str) -> Result<Vec<(DateTime<Utc>, f64)>, ProviderError> {
        let t = self.token()?;
        if !external_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ProviderError::InvalidData { detail: "некорректный server id".into() });
        }
        let now = Utc::now();
        let start = now - crate::services::period_duration(period);
        let path = format!("/servers/{external_id}/player-count-history");
        let (s, e) = (start.to_rfc3339(), now.to_rfc3339());
        let v: Value = self.http.get_json("battlemetrics", Self::url(&path, &[("start", &s), ("stop", &e), ("resolution", "60")])?, Some(&t)).await?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|d| Some((ts(&d["attributes"]["timestamp"])?, d["attributes"]["value"].as_f64()?))).collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fx(name: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/../tests/fixtures/battlemetrics/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
    }
    #[test]
    fn contract_sessions() {
        let s = parse_sessions(&fx("player_sessions.json"));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].server_id.as_deref(), Some("battlemetrics:1000001"));
        assert_eq!(s[0].server_name.as_deref(), Some("Sanitized Server A"));
        assert!(s[1].stop.is_none());
    }
    #[test]
    fn match_requires_exact_steam_id() {
        let v = fx("players_match.json");
        let ok = SteamId64::parse("76561197960287930").unwrap();
        let other = SteamId64::parse("76561197960287931").unwrap();
        assert_eq!(parse_match(&v, &ok), Some(BattleMetricsPlayerId("900001".into())));
        assert_eq!(parse_match(&v, &other), None, "никаких нечётких сопоставлений");
    }
    #[test]
    fn contract_server_roster() {
        let r = parse_server_roster(&fx("server_with_players.json")).unwrap();
        assert_eq!(r.server.external_id, "1000001");
        assert_eq!(r.players.len(), 3);
        assert_eq!(r.players[1], ("900002".to_string(), "Mate One".to_string()));
        assert_eq!(r.server.map.as_deref(), Some("Procedural Map"));
    }
    #[test]
    fn parses_player_id_input() {
        assert_eq!(parse_player_id("123456").unwrap().0, "123456");
        assert_eq!(parse_player_id("https://www.battlemetrics.com/players/987654321").unwrap().0, "987654321");
        assert_eq!(parse_player_id("battlemetrics.com/players/42/").unwrap().0, "42");
        assert!(parse_player_id("https://evil.example/players/1").is_none());
        assert!(parse_player_id("abc").is_none());
    }
    #[test]
    fn contract_player_info_servers_meta() {
        let v = fx("player_info.json");
        let i = parse_player_info(&v).unwrap();
        assert_eq!(i.id.0, "900001");
        assert_eq!(i.steam_id, None, "без RCON-доступа SteamID не раскрывается");
        assert_eq!(i.servers.len(), 2);
        assert!(i.servers.iter().any(|s| s.online && s.server_id == "battlemetrics:1000002"));
        assert_eq!(i.servers.iter().find(|s| s.server_id == "battlemetrics:1000001").unwrap().time_played_secs, Some(36000));
    }
    #[test]
    fn quick_match_without_type_field() {
        let v = serde_json::json!({"data":[{"type":"identifier","attributes":{"identifier":"76561197960287930","lastSeen":"2026-11-01T00:00:00Z"},"relationships":{"player":{"data":{"type":"player","id":"777"}}}}]});
        assert_eq!(parse_match(&v, &SteamId64::parse("76561197960287930").unwrap()).unwrap().0, "777");
    }
    #[test]
    fn name_search_never_assigns_steam_id() {
        let c = parse_player_search(&fx("players_search.json"));
        assert_eq!(c.len(), 2);
        assert!(c.iter().all(|x| x.steam_id.is_none()));
    }
}
