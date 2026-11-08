//! Происхождение данных: источник, время получения, покрытие, статус.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataStatus {
    Verified,
    Stale,
    Partial,
    Unsupported,
    NotConfigured,
    Error,
    Demo,
}

impl DataStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Stale => "stale",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
            Self::NotConfigured => "not_configured",
            Self::Error => "error",
            Self::Demo => "demo",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "verified" => Self::Verified,
            "stale" => Self::Stale,
            "partial" => Self::Partial,
            "unsupported" => Self::Unsupported,
            "not_configured" => Self::NotConfigured,
            "demo" => Self::Demo,
            _ => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub source: String,
    pub fetched_at: Option<DateTime<Utc>>,
    /// Доля покрытия 0..1 (если применимо) или None.
    pub coverage: Option<f64>,
    pub status: DataStatus,
    pub source_url: Option<String>,
    pub note: Option<String>,
}

impl Provenance {
    pub fn new(source: &str, status: DataStatus) -> Self {
        Self { source: source.into(), fetched_at: None, coverage: None, status, source_url: None, note: None }
    }
    pub fn fetched(mut self, at: DateTime<Utc>) -> Self {
        self.fetched_at = Some(at);
        self
    }
    pub fn url(mut self, u: Option<String>) -> Self {
        self.source_url = u;
        self
    }
    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.note = Some(n.into());
        self
    }
    /// Помечает данные устаревшими, если они старше `max_age`.
    pub fn with_staleness(mut self, now: DateTime<Utc>, max_age: chrono::Duration) -> Self {
        if self.status == DataStatus::Verified
            && let Some(at) = self.fetched_at
            && now - at > max_age
        {
            self.status = DataStatus::Stale;
        }
        self
    }
}

/// Значение вместе с происхождением.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sourced<T> {
    pub value: Option<T>,
    pub provenance: Provenance,
}

impl<T> Sourced<T> {
    pub fn some(value: T, provenance: Provenance) -> Self {
        Self { value: Some(value), provenance }
    }
    pub fn none(provenance: Provenance) -> Self {
        Self { value: None, provenance }
    }
}
