//! Баны и объяснимые индикаторы риска (без вердиктов о читерстве).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BanKind {
    /// Steam VAC
    Vac,
    /// Steam Game Ban — стандартный ответ API НЕ указывает игру; это не обязательно бан Rust/EAC.
    Game,
    Community,
    Economy,
    /// Бан на конкретном проекте/сервере из документированного источника.
    Project,
}

impl BanKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vac => "vac",
            Self::Game => "game",
            Self::Community => "community",
            Self::Economy => "economy",
            Self::Project => "project",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "vac" => Self::Vac,
            "game" => Self::Game,
            "community" => Self::Community,
            "economy" => Self::Economy,
            "project" => Self::Project,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanRecord {
    pub kind: BanKind,
    pub provider: String,
    /// Количество (для VAC/Game), если источник сообщает.
    pub count: Option<i64>,
    pub active: bool,
    pub days_since_last: Option<i64>,
    pub reason: Option<String>,
    pub project_name: Option<String>,
    /// None = источник не сообщает об отмене.
    pub revoked: Option<bool>,
    pub source_url: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskIndicator {
    pub id: &'static str,
    pub label: String,
    pub weight: f64,
    pub triggered: bool,
    pub evidence: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskSummary {
    pub enabled: bool,
    pub score: f64,
    pub max_score: f64,
    pub indicators: Vec<RiskIndicator>,
    pub disclaimer: &'static str,
}

pub const RISK_DISCLAIMER: &str =
    "Сумма публичных сигналов, а не вердикт. Ни один индикатор не доказывает использование читов; Game Ban в Steam не указывает конкретную игру.";

pub struct RiskInput<'a> {
    pub bans: &'a [BanRecord],
    pub account_age_days: Option<i64>,
    pub rust_hours: Option<f64>,
    pub profile_public: Option<bool>,
}

pub fn compute_risk(inp: &RiskInput<'_>, enabled: bool) -> RiskSummary {
    let mut v = Vec::new();
    let vac: i64 = inp.bans.iter().filter(|b| b.kind == BanKind::Vac).filter_map(|b| b.count).sum();
    let game: i64 = inp.bans.iter().filter(|b| b.kind == BanKind::Game).filter_map(|b| b.count).sum();
    let recent = inp.bans.iter().filter(|b| matches!(b.kind, BanKind::Vac | BanKind::Game) && b.count.unwrap_or(0) > 0).filter_map(|b| b.days_since_last).min();
    let project_active = inp.bans.iter().filter(|b| b.kind == BanKind::Project && b.active && b.revoked != Some(true)).count();
    v.push(RiskIndicator {
        id: "vac_bans",
        label: "VAC-баны в Steam".into(),
        weight: 2.0,
        triggered: vac > 0,
        evidence: format!("VAC: {vac}"),
        source: "steam".into(),
    });
    v.push(RiskIndicator {
        id: "game_bans",
        label: "Game Ban в Steam (игра не указана)".into(),
        weight: 2.0,
        triggered: game > 0,
        evidence: format!("Game bans: {game}"),
        source: "steam".into(),
    });
    v.push(RiskIndicator {
        id: "recent_ban",
        label: "Последний бан Steam < 365 дней назад".into(),
        weight: 1.0,
        triggered: recent.is_some_and(|d| d < 365),
        evidence: recent.map(|d| format!("{d} дн. назад")).unwrap_or_else(|| "нет данных/банов".into()),
        source: "steam".into(),
    });
    v.push(RiskIndicator {
        id: "project_bans",
        label: "Активные баны на проектах (документированный источник)".into(),
        weight: 1.5,
        triggered: project_active > 0,
        evidence: format!("активных: {project_active}"),
        source: "project_bans".into(),
    });
    v.push(RiskIndicator {
        id: "young_account",
        label: "Аккаунт Steam моложе 90 дней".into(),
        weight: 0.5,
        triggered: inp.account_age_days.is_some_and(|d| d < 90),
        evidence: inp.account_age_days.map(|d| format!("{d} дн.")).unwrap_or_else(|| "дата создания недоступна".into()),
        source: "steam".into(),
    });
    v.push(RiskIndicator {
        id: "low_hours",
        label: "Менее 100 часов в Rust (если часы публичны)".into(),
        weight: 0.5,
        triggered: inp.rust_hours.is_some_and(|h| h < 100.0),
        evidence: inp.rust_hours.map(|h| format!("{h:.0} ч")).unwrap_or_else(|| "часы скрыты".into()),
        source: "steam".into(),
    });
    v.push(RiskIndicator {
        id: "private_profile",
        label: "Закрытый профиль Steam".into(),
        weight: 0.25,
        triggered: inp.profile_public == Some(false),
        evidence: match inp.profile_public {
            Some(true) => "публичный".into(),
            Some(false) => "закрытый".into(),
            None => "неизвестно".into(),
        },
        source: "steam".into(),
    });
    let max: f64 = v.iter().map(|i| i.weight).sum();
    let score = if enabled { v.iter().filter(|i| i.triggered).map(|i| i.weight).sum() } else { 0.0 };
    RiskSummary { enabled, score, max_score: max, indicators: v, disclaimer: RISK_DISCLAIMER }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn risk_is_explainable_and_can_be_disabled() {
        let bans = vec![BanRecord {
            kind: BanKind::Game,
            provider: "steam".into(),
            count: Some(1),
            active: true,
            days_since_last: Some(30),
            reason: None,
            project_name: None,
            revoked: None,
            source_url: None,
            observed_at: Utc::now(),
        }];
        let inp = RiskInput { bans: &bans, account_age_days: None, rust_hours: Some(2000.0), profile_public: Some(true) };
        let r = compute_risk(&inp, true);
        assert_eq!(r.score, 3.0);
        assert!(r.indicators.iter().any(|i| i.id == "game_bans" && i.triggered));
        assert!(!r.indicators.iter().any(|i| i.id == "young_account" && i.triggered), "нет даты — нет сигнала");
        let off = compute_risk(&inp, false);
        assert_eq!(off.score, 0.0);
    }
}
