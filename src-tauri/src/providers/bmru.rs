//! battlemetrics.ru API v1 — ТОЛЬКО серверный уровень (каталог, средний онлайн, вайпы).
//! Документация: https://battlemetrics.ru/api . Не путать с battlemetrics.com.
//! История онлайна сервера НЕ является историей сессий игроков.

use super::error::ProviderError;
use super::{ServerCatalog, ServerInfo};
use crate::infrastructure::http::HttpClient;
use crate::infrastructure::redact::{sanitize_text, sanitize_url};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::sync::Arc;

const BASE: &str = "https://battlemetrics.ru/api/v1";
pub type KeyFn = Arc<dyn Fn() -> Option<String> + Send + Sync>;

pub struct BmRuProvider {
    http: HttpClient,
    key: KeyFn,
}

fn ts(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&Utc))
}

fn id_str(v: &Value) -> Option<String> {
    v.as_i64().map(|i| i.to_string()).or_else(|| v.as_str().filter(|s| s.bytes().all(|b| b.is_ascii_digit())).map(str::to_string))
}

pub fn parse_server(s: &Value) -> Option<ServerInfo> {
    let id = id_str(&s["id"])?;
    Some(ServerInfo {
        provider: "battlemetrics_ru",
        name: sanitize_text(s["name"].as_str().unwrap_or("?"), 120),
        address: match (s["address"].as_str(), s["port"].as_i64()) {
            (Some(a), Some(p)) => Some(format!("{}:{p}", sanitize_text(a, 64))),
            (Some(a), None) => Some(sanitize_text(a, 64)),
            _ => None,
        },
        country: s["country"].as_str().map(|c| sanitize_text(c, 4)),
        map: s["map"].as_str().map(|c| sanitize_text(c, 64)),
        // players = null, если trust != confirmed — сохраняем null, а не 0.
        players: s["players"].as_i64(),
        max_players: s["max_players"].as_i64(),
        queue: s["queue"].as_i64(),
        online: s["online"].as_bool(),
        trust: s["trust"].as_str().map(|c| sanitize_text(c, 16)),
        last_wipe_at: ts(&s["last_wipe"]),
        next_wipe_at: None,
        source_url: s["url"].as_str().and_then(|u| sanitize_url(u, &["battlemetrics.ru"])),
        external_id: id,
    })
}

pub fn parse_servers(body: &Value) -> Vec<ServerInfo> {
    body["servers"].as_array().map(|a| a.iter().filter_map(parse_server).collect()).unwrap_or_default()
}

pub fn parse_history(body: &Value) -> Vec<(DateTime<Utc>, f64)> {
    body["points"].as_array().map(|a| a.iter().filter_map(|p| Some((ts(&p["t"])?, p["players"].as_f64()?))).collect()).unwrap_or_default()
}

/// Ответ /wipes: массив записей (на верхнем уровне или в поле wipes/items/data).
pub fn parse_wipes(body: &Value) -> Vec<(String, DateTime<Utc>, bool)> {
    let arr = body.as_array().or_else(|| body["wipes"].as_array()).or_else(|| body["items"].as_array()).or_else(|| body["data"].as_array());
    arr.map(|a| a.iter().filter_map(|w| Some((id_str(&w["server_id"])?, ts(&w["at"])?, w["fact"].as_bool().unwrap_or(false)))).collect()).unwrap_or_default()
}

impl BmRuProvider {
    pub fn new(http: HttpClient, key: KeyFn) -> Self {
        Self { http, key }
    }
    pub fn is_configured(&self) -> bool {
        (self.key)().is_some()
    }
    fn key(&self) -> Result<String, ProviderError> {
        (self.key)()
            .ok_or_else(|| ProviderError::NotConfigured { hint: "Добавьте бесплатный ключ battlemetrics.ru (bm_live_…)".into() })
    }
    fn url(path: &str, params: &[(&str, &str)]) -> Result<reqwest::Url, ProviderError> {
        reqwest::Url::parse_with_params(&format!("{BASE}{path}"), params).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })
    }
    pub async fn test(&self) -> Result<String, ProviderError> {
        let k = self.key()?;
        let v: Value = self.http.get_json("battlemetrics_ru", Self::url("/servers", &[("game", "rust"), ("per_page", "20")])?, Some(&k)).await?;
        Ok(format!("Ключ принят, серверов Rust: {}", v["total"].as_i64().unwrap_or(0)))
    }
    /// Предстоящие (fact=false) и прошедшие (fact=true) вайпы.
    pub async fn wipes(&self, when: &str) -> Result<Vec<(String, DateTime<Utc>, bool)>, ProviderError> {
        let k = self.key()?;
        let when = if when == "past" { "past" } else { "upcoming" };
        let v: Value = self.http.get_json("battlemetrics_ru", Self::url("/wipes", &[("game", "rust"), ("when", when), ("limit", "200")])?, Some(&k)).await?;
        Ok(parse_wipes(&v))
    }
}

#[async_trait]
impl ServerCatalog for BmRuProvider {
    fn id(&self) -> &'static str {
        "battlemetrics_ru"
    }
    async fn search_servers(&self, query: Option<&str>) -> Result<Vec<ServerInfo>, ProviderError> {
        let k = self.key()?;
        let mut params = vec![("game", "rust"), ("per_page", "50"), ("sort", "players")];
        let q = query.map(|q| q.trim()).filter(|q| (3..=60).contains(&q.chars().count()));
        if let Some(q) = q {
            params.push(("q", q));
        }
        let v: Value = self.http.get_json("battlemetrics_ru", Self::url("/servers", &params)?, Some(&k)).await?;
        Ok(parse_servers(&v))
    }
    async fn population(&self, external_id: &str, period: &str) -> Result<Vec<(DateTime<Utc>, f64)>, ProviderError> {
        let k = self.key()?;
        if !external_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ProviderError::InvalidData { detail: "некорректный server id".into() });
        }
        let period = match period {
            "24h" | "7d" | "30d" => period,
            _ => "7d",
        };
        let v: Value = self.http.get_json("battlemetrics_ru", Self::url(&format!("/servers/{external_id}/history"), &[("period", period)])?, Some(&k)).await?;
        Ok(parse_history(&v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fx(name: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/../tests/fixtures/bmru/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
    }
    #[test]
    fn contract_servers_keep_null_players() {
        let s = parse_servers(&fx("servers.json"));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].players, Some(180));
        assert_eq!(s[1].players, None, "trust != confirmed → players = null, не 0");
        assert_eq!(s[1].trust.as_deref(), Some("suspicious"));
        assert!(s[0].last_wipe_at.is_some());
    }
    #[test]
    fn contract_history_and_wipes() {
        assert_eq!(parse_history(&fx("history.json")).len(), 3);
        let w = parse_wipes(&fx("wipes.json"));
        assert_eq!(w.len(), 1);
        assert!(!w[0].2);
    }
}
