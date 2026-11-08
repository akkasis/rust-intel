//! Несекретная конфигурация приложения (config.json в каталоге данных).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Demo,
    Live,
}

impl Mode {
    pub fn db_file(&self) -> &'static str {
        match self {
            Self::Demo => "rid-demo.sqlite",
            Self::Live => "rid-live.sqlite",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NotificationSettings {
    pub windows: bool,
    pub telegram: bool,
    pub telegram_chat_id: Option<String>,
    /// "HH:MM" в часовом поясе настроек
    pub quiet_start: Option<String>,
    pub quiet_end: Option<String>,
    /// Включённые типы событий
    pub events: BTreeMap<String, bool>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        let mut events = BTreeMap::new();
        for k in crate::domain::tracking::EventKind::ALL {
            let on = !matches!(k, crate::domain::tracking::EventKind::SourceRestored);
            events.insert(k.as_str().to_string(), on);
        }
        Self { windows: false, telegram: false, telegram_chat_id: None, quiet_start: None, quiet_end: None, events }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub timezone: String,
    pub autostart: bool,
    pub close_to_tray: bool,
    pub retention_days: i64,
    pub poll_interval_secs: u64,
    pub tracking_paused: bool,
    pub risk_indicators: bool,
    pub reduced_motion: String,
    pub density: String,
    /// Автоматически включать наблюдение за серверами привязанных игроков BM.
    pub auto_watch_servers: bool,
    pub notifications: NotificationSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            language: "ru".into(),
            timezone: "UTC".into(),
            autostart: false,
            close_to_tray: false,
            retention_days: 90,
            poll_interval_secs: 120,
            tracking_paused: false,
            risk_indicators: true,
            reduced_motion: "system".into(),
            density: "compact".into(),
            auto_watch_servers: true,
            notifications: NotificationSettings::default(),
        }
    }
}

impl AppSettings {
    pub fn validate(mut self) -> Result<Self, String> {
        if self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(format!("Неизвестный часовой пояс: {}", self.timezone));
        }
        if !["dark", "light", "system"].contains(&self.theme.as_str()) {
            self.theme = "dark".into();
        }
        self.language = "ru".into();
        self.retention_days = self.retention_days.clamp(7, 3650);
        self.poll_interval_secs = self.poll_interval_secs.clamp(60, 3600);
        if let Some(c) = &self.notifications.telegram_chat_id {
            let c = c.trim();
            let ok = !c.is_empty() && c.len() <= 64 && (c.trim_start_matches('-').bytes().all(|b| b.is_ascii_digit()) || c.starts_with('@'));
            if !ok {
                return Err("Telegram chat ID: число (например, 123456789 или -100…) или @channel".into());
            }
            self.notifications.telegram_chat_id = Some(c.to_string());
        }
        for q in [&self.notifications.quiet_start, &self.notifications.quiet_end].into_iter().flatten() {
            if chrono::NaiveTime::parse_from_str(q, "%H:%M").is_err() {
                return Err("Тихие часы: формат ЧЧ:ММ".into());
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    /// None = первый запуск, режим ещё не выбран.
    pub mode: Option<Mode>,
    pub settings: AppSettings,
}

pub fn config_path(dir: &Path) -> PathBuf {
    dir.join("config.json")
}

pub fn load(dir: &Path) -> AppConfig {
    std::fs::read_to_string(config_path(dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(dir: &Path, cfg: &AppConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("config.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(cfg).unwrap_or_default())?;
    std::fs::rename(tmp, config_path(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_settings() {
        let mut s = AppSettings { timezone: "Europe/Moscow".into(), poll_interval_secs: 5, ..Default::default() };
        s = s.validate().unwrap();
        assert_eq!(s.poll_interval_secs, 60, "интервал опроса не ниже безопасного минимума");
        assert!(AppSettings { timezone: "Mars/Base".into(), ..Default::default() }.validate().is_err());
        let mut bad = AppSettings::default();
        bad.notifications.telegram_chat_id = Some("1; DROP".into());
        assert!(bad.validate().is_err());
    }
    #[test]
    fn roundtrip_file() {
        let d = tempfile::tempdir().unwrap();
        let cfg = AppConfig { mode: Some(Mode::Demo), settings: AppSettings::default() };
        save(d.path(), &cfg).unwrap();
        assert_eq!(load(d.path()).mode, Some(Mode::Demo));
    }
}
