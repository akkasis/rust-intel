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
        (a["type"] == "steamID" && a["identifier"].as_str() == Some(steam_id.as_str()))
            .then(|| d["relationships"]["player"]["data"]["id"].as_str().map(|s| BattleMetricsPlayerId(s.to_string())))
            .flatten()
    })
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
        let v: Value = self.http.post_json("battlemetrics", Self::url("/players/match", &[])?, Some(&t), &body).await?;
        Ok(parse_match(&v, steam_id))
    }
    pub async fn search_by_name(&self, name: &str) -> Result<Vec<NameCandidate>, ProviderError> {
        let t = self.token()?;
        let v: Value = self
            .http
            .get_json("battlemetrics", Self::url("/players", &[("filter[search]", name), ("filter[server][game]", "rust"), ("page[size]", "10")])?, Some(&t))
            .await?;
        Ok(parse_player_search(&v))
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
                out.push(Presence { steam_id: id.clone(), state: PresenceState::Unknown, server_id: None, server_name: None, persona_name: None });
                continue;
            };
            let s = self.provider.sessions(bm).await?;
            let open = s.iter().find(|x| x.stop.is_none());
            out.push(Presence {
                steam_id: id.clone(),
                state: if open.is_some() { PresenceState::Online } else { PresenceState::Offline },
                server_id: open.and_then(|o| o.server_id.clone()),
                server_name: open.and_then(|o| o.server_name.clone()),
                persona_name: s.first().and_then(|x| x.player_name.clone()),
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
    fn name_search_never_assigns_steam_id() {
        let c = parse_player_search(&fx("players_search.json"));
        assert_eq!(c.len(), 2);
        assert!(c.iter().all(|x| x.steam_id.is_none()));
    }
}
