//! HTTP-клиент: allowlist хостов (защита от SSRF), таймауты, лимит частоты на провайдера,
//! ретраи с экспоненциальной задержкой и джиттером, учёт Retry-After / 429 / 403.

use crate::infrastructure::redact::redact;
use crate::providers::error::ProviderError;
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub const ALLOWED_HOSTS: &[&str] = &["api.steampowered.com", "api.battlemetrics.com", "battlemetrics.ru", "api.telegram.org"];

#[derive(Clone, Debug)]
pub struct ProviderPolicy {
    /// Минимальный интервал между запросами.
    pub min_interval: Duration,
    pub timeout: Duration,
    pub max_retries: u32,
    /// Максимальное ожидание по Retry-After, дольше — сразу возвращаем RateLimited.
    pub max_retry_after: Duration,
}

impl Default for ProviderPolicy {
    fn default() -> Self {
        Self { min_interval: Duration::from_millis(1100), timeout: Duration::from_secs(15), max_retries: 2, max_retry_after: Duration::from_secs(10) }
    }
}

#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    last_call: Arc<Mutex<HashMap<String, Instant>>>,
    policies: Arc<HashMap<String, ProviderPolicy>>,
}

fn jitter_ms(max: u64) -> u64 {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0);
    if max == 0 { 0 } else { n % max }
}

pub fn check_host(url: &reqwest::Url) -> Result<(), ProviderError> {
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https" || !ALLOWED_HOSTS.contains(&host) {
        return Err(ProviderError::InvalidData { detail: format!("хост не в allowlist: {host}") });
    }
    Ok(())
}

impl HttpClient {
    pub fn new() -> Self {
        let inner = reqwest::Client::builder()
            .user_agent(concat!("RustIntelligenceDesktop/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client");
        let mut p = HashMap::new();
        // Steam Web API: общий дневной лимит 100 000 вызовов/ключ; держим ≤ 1 rps.
        p.insert("steam".into(), ProviderPolicy::default());
        // BattleMetrics.com: публичный лимит ~60/мин (токен — выше); держим 1 rps.
        p.insert("battlemetrics".into(), ProviderPolicy::default());
        // battlemetrics.ru: 60 запросов/мин/ключ по документации.
        p.insert("battlemetrics_ru".into(), ProviderPolicy::default());
        p.insert("telegram".into(), ProviderPolicy { min_interval: Duration::from_millis(1100), ..Default::default() });
        Self { inner, last_call: Arc::new(Mutex::new(HashMap::new())), policies: Arc::new(p) }
    }

    fn policy(&self, provider: &str) -> ProviderPolicy {
        self.policies.get(provider).cloned().unwrap_or_default()
    }

    async fn throttle(&self, provider: &str, min: Duration) {
        loop {
            let wait = {
                let mut m = self.last_call.lock().await;
                let now = Instant::now();
                match m.get(provider) {
                    Some(last) if now.duration_since(*last) < min => Some(min - now.duration_since(*last)),
                    _ => {
                        m.insert(provider.to_string(), now);
                        None
                    }
                }
            };
            match wait {
                Some(w) => tokio::time::sleep(w).await,
                None => return,
            }
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, provider: &str, url: reqwest::Url, bearer: Option<&str>) -> Result<T, ProviderError> {
        let body = self.request(provider, Method::GET, url, bearer, None).await?;
        serde_json::from_slice(&body).map_err(|e| ProviderError::InvalidData { detail: format!("JSON: {e}") })
    }

    pub async fn post_json<T: DeserializeOwned>(
        &self,
        provider: &str,
        url: reqwest::Url,
        bearer: Option<&str>,
        json: &serde_json::Value,
    ) -> Result<T, ProviderError> {
        let body = self.request(provider, Method::POST, url, bearer, Some(json)).await?;
        serde_json::from_slice(&body).map_err(|e| ProviderError::InvalidData { detail: format!("JSON: {e}") })
    }

    async fn request(
        &self,
        provider: &str,
        method: Method,
        url: reqwest::Url,
        bearer: Option<&str>,
        json: Option<&serde_json::Value>,
    ) -> Result<Vec<u8>, ProviderError> {
        check_host(&url)?;
        let pol = self.policy(provider);
        let safe_url = redact(url.as_str());
        let mut attempt = 0u32;
        loop {
            self.throttle(provider, pol.min_interval).await;
            let mut rb = self.inner.request(method.clone(), url.clone()).timeout(pol.timeout).header("Accept", "application/json");
            if let Some(b) = bearer {
                rb = rb.bearer_auth(b);
            }
            if let Some(j) = json {
                rb = rb.json(j);
            }
            let started = Instant::now();
            let res = rb.send().await;
            let err = match res {
                Ok(resp) => {
                    let status = resp.status();
                    tracing::debug!(provider, url = %safe_url, status = status.as_u16(), ms = started.elapsed().as_millis() as u64, "http");
                    let retry_after = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok());
                    if status.is_success() {
                        // ограничиваем размер ответа 8 МБ
                        let bytes = resp.bytes().await.map_err(|e| ProviderError::Unavailable { detail: redact(&e.to_string()), retry_after_secs: None })?;
                        if bytes.len() > 8 * 1024 * 1024 {
                            return Err(ProviderError::InvalidData { detail: "ответ слишком большой".into() });
                        }
                        return Ok(bytes.to_vec());
                    }
                    let detail = resp.text().await.unwrap_or_default();
                    let detail = redact(&detail.chars().take(300).collect::<String>());
                    match status {
                        StatusCode::TOO_MANY_REQUESTS => {
                            if let Some(ra) = retry_after
                                && Duration::from_secs(ra) <= pol.max_retry_after
                                && attempt < pol.max_retries
                            {
                                tokio::time::sleep(Duration::from_secs(ra)).await;
                                attempt += 1;
                                continue;
                            }
                            return Err(ProviderError::RateLimited { retry_after_secs: retry_after });
                        }
                        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                            return Err(ProviderError::Forbidden { status: status.as_u16(), detail });
                        }
                        StatusCode::NOT_FOUND => return Err(ProviderError::NotFound { what: safe_url.clone() }),
                        s if s.is_server_error() => ProviderError::Unavailable { detail: format!("HTTP {}", s.as_u16()), retry_after_secs: retry_after },
                        s => return Err(ProviderError::InvalidData { detail: format!("HTTP {}: {detail}", s.as_u16()) }),
                    }
                }
                Err(e) if e.is_timeout() => ProviderError::Timeout,
                Err(e) => ProviderError::Unavailable { detail: redact(&e.to_string()), retry_after_secs: None },
            };
            if attempt >= pol.max_retries {
                return Err(err);
            }
            attempt += 1;
            let backoff = 500u64 * 2u64.pow(attempt) + jitter_ms(400);
            tracing::warn!(provider, url = %safe_url, attempt, backoff_ms = backoff, error = %err, "повтор запроса");
            tokio::time::sleep(Duration::from_millis(backoff)).await;
        }
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_foreign_hosts() {
        assert!(check_host(&reqwest::Url::parse("https://api.steampowered.com/x").unwrap()).is_ok());
        assert!(check_host(&reqwest::Url::parse("https://169.254.169.254/latest").unwrap()).is_err());
        assert!(check_host(&reqwest::Url::parse("http://api.steampowered.com/x").unwrap()).is_err());
        assert!(check_host(&reqwest::Url::parse("https://localhost/x").unwrap()).is_err());
    }
}
