pub mod analytics_service;
pub mod export;
pub mod notifications;
pub mod player_service;
pub mod server_service;
pub mod tracker_service;

use crate::domain::analytics::{Interval, SessionBounds};
use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::parse_ts;
use crate::persistence::models::{CoverageRow, SessionRow};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

pub fn period_duration(p: &str) -> Duration {
    match p {
        "24h" => Duration::hours(24),
        "7d" => Duration::days(7),
        "30d" => Duration::days(30),
        "90d" => Duration::days(90),
        _ => Duration::days(7),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RangeSpec {
    pub preset: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl RangeSpec {
    pub fn resolve(&self, now: DateTime<Utc>) -> AppResult<Interval> {
        if self.preset == "custom" {
            let f = self.from.as_deref().and_then(parse_ts).ok_or_else(|| AppError::validation("Укажите начало периода"))?;
            let t = self.to.as_deref().and_then(parse_ts).unwrap_or(now).min(now);
            if t - f > Duration::days(400) {
                return Err(AppError::validation("Период не длиннее 400 дней"));
            }
            return Interval::new(f, t).ok_or_else(|| AppError::validation("Начало периода должно быть раньше конца"));
        }
        if !["24h", "7d", "30d", "90d"].contains(&self.preset.as_str()) {
            return Err(AppError::validation("Неизвестный период"));
        }
        Interval::new(now - period_duration(&self.preset), now).ok_or_else(|| AppError::validation("Пустой период"))
    }
}

pub fn to_bounds(rows: &[SessionRow]) -> Vec<SessionBounds> {
    rows.iter()
        .filter_map(|r| {
            Some(SessionBounds {
                start_lower: parse_ts(&r.start_lower)?,
                start_upper: parse_ts(&r.start_upper)?,
                end_lower: parse_ts(&r.end_lower)?,
                end_upper: r.end_upper.as_deref().and_then(parse_ts),
                server_id: r.server_id.clone(),
            })
        })
        .collect()
}

pub fn to_intervals(rows: &[CoverageRow]) -> Vec<Interval> {
    rows.iter().filter_map(|r| Interval::new(parse_ts(&r.start_at)?, parse_ts(&r.end_at)?)).collect()
}
