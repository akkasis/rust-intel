//! Уведомления: Windows (через tauri-plugin-notification) и Telegram Bot API (ключи пользователя).
//! Опционально, без дублей (флаг notified), с тихими часами и фильтром по типам событий.

use crate::config::AppSettings;
use crate::domain::tracking::EventKind;
use crate::infrastructure::clock::parse_ts;
use crate::infrastructure::secrets::SecretKey;
use crate::persistence::models::EventRow;
use crate::persistence::repo;
use crate::providers::error::ProviderError;
use crate::state::Ctx;
use chrono::{NaiveTime, Utc};
use chrono_tz::Tz;

pub fn in_quiet_hours(s: &AppSettings, at: chrono::DateTime<Utc>) -> bool {
    let (Some(a), Some(b)) = (&s.notifications.quiet_start, &s.notifications.quiet_end) else { return false };
    let (Ok(a), Ok(b)) = (NaiveTime::parse_from_str(a, "%H:%M"), NaiveTime::parse_from_str(b, "%H:%M")) else { return false };
    let tz: Tz = s.timezone.parse().unwrap_or(chrono_tz::UTC);
    let local = at.with_timezone(&tz).time();
    if a <= b { local >= a && local < b } else { local >= a || local < b }
}

pub fn format_event(e: &EventRow, tz: Tz) -> (String, String) {
    let kind = EventKind::parse(&e.kind);
    let title = kind.map(|k| k.title_ru()).unwrap_or("Событие").to_string();
    let who = e.persona_name.clone().or_else(|| e.steam_id.clone()).unwrap_or_else(|| "Источник".into());
    let fmt = |s: &str| parse_ts(s).map(|t| t.with_timezone(&tz).format("%d.%m %H:%M").to_string()).unwrap_or_default();
    let when = if e.approximate && e.occurred_lower != e.occurred_upper {
        format!("≈ {}–{}", fmt(&e.occurred_lower), fmt(&e.occurred_upper))
    } else {
        fmt(&e.occurred_upper)
    };
    let server = e.server_name.clone().or_else(|| e.server_id.clone()).map(|s| format!(" · {s}")).unwrap_or_default();
    (format!("{who}: {title}"), format!("{when}{server} · источник: {}", e.provider))
}

pub async fn send_telegram(ctx: &Ctx, chat_id: &str, text: &str) -> Result<(), ProviderError> {
    let token = ctx
        .secrets
        .get(SecretKey::TelegramBotToken)
        .ok_or_else(|| ProviderError::NotConfigured { hint: "Добавьте токен Telegram-бота".into() })?;
    if !token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b':' || b == b'_' || b == b'-') {
        return Err(ProviderError::InvalidData { detail: "некорректный формат токена".into() });
    }
    let url =
        reqwest::Url::parse(&format!("https://api.telegram.org/bot{token}/sendMessage")).map_err(|e| ProviderError::InvalidData { detail: e.to_string() })?;
    let body = serde_json::json!({"chat_id": chat_id, "text": text, "disable_web_page_preview": true});
    let v: serde_json::Value = ctx.http.post_json("telegram", url, None, &body).await?;
    if v["ok"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err(ProviderError::InvalidData { detail: v["description"].as_str().unwrap_or("Telegram error").chars().take(200).collect() })
    }
}

/// Отбор событий для уведомления; возвращает (событие, заголовок, текст).
pub async fn pending(ctx: &Ctx, ids: &[i64]) -> Vec<(EventRow, String, String)> {
    let s = &ctx.settings;
    if (!s.notifications.windows && !s.notifications.telegram) || in_quiet_hours(s, Utc::now()) {
        return vec![];
    }
    let tracked = repo::list_tracked(&ctx.pool).await.unwrap_or_default();
    let events = repo::list_events(&ctx.pool, None, None, false, 200, 0).await.unwrap_or_default();
    let tz = ctx.tz();
    events
        .into_iter()
        .filter(|e| ids.contains(&e.id) && !e.notified)
        .filter(|e| s.notifications.events.get(&e.kind).copied().unwrap_or(false))
        .filter(|e| match &e.steam_id {
            Some(sid) => tracked.iter().any(|t| &t.steam_id == sid && t.notify),
            None => true,
        })
        .map(|e| {
            let (t, b) = format_event(&e, tz);
            (e, t, b)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quiet_hours_wrap_midnight() {
        let mut s = AppSettings { timezone: "Europe/Moscow".into(), ..Default::default() };
        s.notifications.quiet_start = Some("23:00".into());
        s.notifications.quiet_end = Some("08:00".into());
        let t = parse_ts("2026-11-01T21:30:00Z").unwrap(); // 00:30 MSK
        assert!(in_quiet_hours(&s, t));
        let t = parse_ts("2026-11-01T09:30:00Z").unwrap(); // 12:30 MSK
        assert!(!in_quiet_hours(&s, t));
    }
}
