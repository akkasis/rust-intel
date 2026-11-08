//! Узкая граница команд Tauri. Секреты во frontend не возвращаются никогда.

use crate::config::{AppSettings, Mode};
use crate::domain::bans::RiskSummary;
use crate::domain::ids::{ParsedInput, SteamId64, parse_input};
use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::{fmt_ts, now};
use crate::infrastructure::redact::mask;
use crate::infrastructure::secrets::{SecretBackend, SecretKey};
use crate::persistence::models::*;
use crate::persistence::{db, repo};
use crate::providers::{AccessStatus, ProviderDescriptor, descriptors};
use crate::services::analytics_service::{ActivityReport, SquadReport};
use crate::services::export::ImportResult;
use crate::services::player_service::{PlayerProfile, SearchOutcome};
use crate::services::server_service::{PopulationReport, RefreshResult};
use crate::services::tracker_service::TrackerStatus;
use crate::services::{RangeSpec, analytics_service, export, notifications, player_service, server_service, tracker_service};
use crate::state::AppState;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

// ------------------------------------------------------------------- app

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub mode: Option<Mode>,
    pub first_run: bool,
    pub data_dir: String,
    pub settings: AppSettings,
    pub secret_backend: SecretBackend,
    pub schema_version: i64,
    pub platform: String,
}

#[tauri::command]
pub async fn get_app_info(state: St<'_>) -> AppResult<AppInfo> {
    tracing::info!("интерфейс подключён к ядру (get_app_info)");
    let cfg = state.config().await;
    let schema_version = match state.ctx().await {
        Ok(c) => db::schema_version(&c.pool).await,
        Err(_) => 0,
    };
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        mode: cfg.mode,
        first_run: cfg.mode.is_none(),
        data_dir: state.data_dir.display().to_string(),
        settings: cfg.settings,
        secret_backend: state.secrets.backend(),
        schema_version,
        platform: std::env::consts::OS.into(),
    })
}

#[tauri::command]
pub async fn choose_mode(state: St<'_>, mode: Mode, timezone: Option<String>) -> AppResult<()> {
    if let Some(tz) = timezone {
        let mut cfg = state.config().await;
        if cfg.mode.is_none() && tz.parse::<chrono_tz::Tz>().is_ok() {
            cfg.settings.timezone = tz;
            state.update_settings(cfg.settings).await?;
        }
    }
    state.set_mode(mode).await
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, state: St<'_>, settings: AppSettings) -> AppResult<AppSettings> {
    let old = state.config().await.settings;
    let s = state.update_settings(settings).await?;
    if old.autostart != s.autostart {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        let r = if s.autostart { al.enable() } else { al.disable() };
        if let Err(e) = r {
            tracing::warn!(error = %e, "автозапуск");
        }
    }
    crate::tray::refresh_menu(&app, s.tracking_paused);
    Ok(s)
}

// ----------------------------------------------------------- search/players

#[tauri::command]
pub fn parse_identifier(input: String) -> Result<ParsedInput, AppError> {
    Ok(parse_input(&input)?)
}

#[tauri::command]
pub async fn search_player(state: St<'_>, input: String) -> AppResult<SearchOutcome> {
    if input.len() > 300 {
        return Err(AppError::validation("Слишком длинный ввод"));
    }
    player_service::search(&state.ctx().await?, &input).await
}

#[tauri::command]
pub async fn recent_searches(state: St<'_>) -> AppResult<Vec<SearchHistoryRow>> {
    Ok(repo::recent_searches(&state.ctx().await?.pool).await?)
}

#[tauri::command]
pub async fn list_players(
    state: St<'_>,
    query: Option<String>,
    favorites_only: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<PlayerRow>> {
    let c = state.ctx().await?;
    Ok(repo::list_players(&c.pool, query.as_deref(), favorites_only.unwrap_or(false), limit.unwrap_or(200).clamp(1, 1000), offset.unwrap_or(0).max(0)).await?)
}

#[tauri::command]
pub async fn get_player_profile(state: St<'_>, steam_id: String) -> AppResult<PlayerProfile> {
    player_service::get_profile(&state.ctx().await?, &steam_id).await
}

#[tauri::command]
pub async fn refresh_player(state: St<'_>, steam_id: String) -> AppResult<Vec<player_service::Notice>> {
    let c = state.ctx().await?;
    let id = SteamId64::parse(&steam_id)?;
    let mut notices = vec![];
    player_service::refresh_player(&c, &id, &mut notices).await?;
    Ok(notices)
}

#[tauri::command]
pub async fn search_battlemetrics(state: St<'_>, name: String) -> AppResult<Vec<player_service::SearchCandidate>> {
    player_service::search_bm(&state.ctx().await?, &name).await
}

#[tauri::command]
pub async fn link_battlemetrics(state: St<'_>, steam_id: String, input: String) -> AppResult<Vec<player_service::Notice>> {
    if input.len() > 200 {
        return Err(AppError::validation("Слишком длинный ввод"));
    }
    let r = player_service::link_bm(&state.ctx().await?, &steam_id, &input).await;
    state.tracker_wake.notify_one();
    r
}

#[tauri::command]
pub async fn unlink_battlemetrics(state: St<'_>, steam_id: String) -> AppResult<()> {
    player_service::unlink_bm(&state.ctx().await?, &steam_id).await
}

#[tauri::command]
pub async fn set_favorite(state: St<'_>, steam_id: String, favorite: bool) -> AppResult<()> {
    let c = state.ctx().await?;
    player_service::ensure_known_player(&c, &steam_id).await?;
    Ok(repo::set_favorite(&c.pool, &steam_id, favorite).await?)
}

#[tauri::command]
pub async fn set_player_notes(state: St<'_>, steam_id: String, notes: Option<String>) -> AppResult<()> {
    let c = state.ctx().await?;
    let notes = notes.map(|n| crate::infrastructure::redact::sanitize_text(&n, 2000));
    Ok(repo::set_notes(&c.pool, &steam_id, notes.as_deref()).await?)
}

#[tauri::command]
pub async fn delete_player(state: St<'_>, steam_id: String) -> AppResult<()> {
    let c = state.ctx().await?;
    SteamId64::parse(&steam_id)?;
    Ok(repo::delete_player(&c.pool, &steam_id).await?)
}

#[tauri::command]
pub async fn purge_player_history(state: St<'_>, steam_id: String) -> AppResult<()> {
    let c = state.ctx().await?;
    SteamId64::parse(&steam_id)?;
    Ok(repo::purge_player_history(&c.pool, &steam_id).await?)
}

#[tauri::command]
pub async fn get_activity(state: St<'_>, steam_id: String, range: RangeSpec, timezone: Option<String>, server_id: Option<String>) -> AppResult<ActivityReport> {
    analytics_service::activity(&state.ctx().await?, &steam_id, &range, timezone.as_deref(), server_id.as_deref()).await
}

// ------------------------------------------------------------------ squads

fn valid_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

#[tauri::command]
pub async fn list_squads(state: St<'_>) -> AppResult<Vec<SquadRow>> {
    Ok(repo::list_squads(&state.ctx().await?.pool).await?)
}

#[tauri::command]
pub async fn save_squad(state: St<'_>, id: Option<i64>, name: String, color: String, notes: Option<String>) -> AppResult<i64> {
    let c = state.ctx().await?;
    let name = crate::infrastructure::redact::sanitize_text(&name, 64);
    if name.is_empty() {
        return Err(AppError::validation("Название отряда не может быть пустым"));
    }
    let color = if valid_color(&color) { color } else { "#2dd4bf".into() };
    let notes = notes.map(|n| crate::infrastructure::redact::sanitize_text(&n, 1000));
    match id {
        Some(id) => {
            repo::update_squad(&c.pool, id, &name, &color, notes.as_deref()).await?;
            Ok(id)
        }
        None => Ok(repo::create_squad(&c.pool, &name, &color, notes.as_deref()).await?),
    }
}

#[tauri::command]
pub async fn delete_squad(state: St<'_>, id: i64) -> AppResult<()> {
    Ok(repo::delete_squad(&state.ctx().await?.pool, id).await?)
}

#[tauri::command]
pub async fn add_squad_member(state: St<'_>, id: i64, steam_id: String, provenance: Option<String>) -> AppResult<()> {
    let c = state.ctx().await?;
    player_service::ensure_known_player(&c, &steam_id).await?;
    let n: i64 = repo::list_squad_members(&c.pool, id).await?.len() as i64;
    if n >= 20 {
        return Err(AppError::validation("В отряде не больше 20 участников"));
    }
    let prov = match provenance.as_deref() {
        Some("accepted_suggestion") => "accepted_suggestion",
        _ => "manual",
    };
    Ok(repo::add_squad_member(&c.pool, id, &steam_id, prov).await?)
}

#[tauri::command]
pub async fn remove_squad_member(state: St<'_>, id: i64, steam_id: String) -> AppResult<()> {
    Ok(repo::remove_squad_member(&state.ctx().await?.pool, id, &steam_id).await?)
}

#[tauri::command]
pub async fn get_squad_report(state: St<'_>, id: i64, range: RangeSpec, timezone: Option<String>) -> AppResult<SquadReport> {
    analytics_service::squad_report(&state.ctx().await?, id, &range, timezone.as_deref()).await
}

// ----------------------------------------------------------------- tracker

#[tauri::command]
pub async fn list_tracked(state: St<'_>) -> AppResult<Vec<TrackedRow>> {
    Ok(repo::list_tracked(&state.ctx().await?.pool).await?)
}

#[tauri::command]
pub async fn set_tracking(state: St<'_>, steam_id: String, enabled: bool) -> AppResult<()> {
    let c = state.ctx().await?;
    player_service::ensure_known_player(&c, &steam_id).await?;
    repo::set_tracked(&c.pool, &steam_id, enabled).await?;
    state.tracker_wake.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn remove_tracking(state: St<'_>, steam_id: String) -> AppResult<()> {
    Ok(repo::remove_tracked(&state.ctx().await?.pool, &steam_id).await?)
}

#[tauri::command]
pub async fn set_tracking_notify(state: St<'_>, steam_id: String, notify: bool) -> AppResult<()> {
    Ok(repo::set_tracked_notify(&state.ctx().await?.pool, &steam_id, notify).await?)
}

#[tauri::command]
pub async fn set_tracking_paused(app: AppHandle, state: St<'_>, paused: bool) -> AppResult<()> {
    let mut s = state.config().await.settings;
    s.tracking_paused = paused;
    state.update_settings(s).await?;
    crate::tray::refresh_menu(&app, paused);
    Ok(())
}

#[tauri::command]
pub async fn get_tracker_status() -> AppResult<TrackerStatus> {
    Ok(tracker_service::STATUS.read().await.clone())
}

#[tauri::command]
pub async fn poll_now(app: AppHandle, state: St<'_>) -> AppResult<usize> {
    let c = state.ctx().await?;
    let ev = tracker_service::poll_once(&c).await?;
    let n = ev.len();
    if n > 0 {
        dispatch_events(&app, c, ev).await;
    }
    Ok(n)
}

#[tauri::command]
pub async fn list_events(
    state: St<'_>,
    steam_id: Option<String>,
    kind: Option<String>,
    unread_only: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<EventRow>> {
    let c = state.ctx().await?;
    Ok(repo::list_events(
        &c.pool,
        steam_id.as_deref(),
        kind.as_deref(),
        unread_only.unwrap_or(false),
        limit.unwrap_or(100).clamp(1, 1000),
        offset.unwrap_or(0).max(0),
    )
    .await?)
}

#[tauri::command]
pub async fn mark_events_read(state: St<'_>, ids: Option<Vec<i64>>) -> AppResult<()> {
    Ok(repo::mark_events_read(&state.ctx().await?.pool, ids.as_deref()).await?)
}

/// Отправка уведомлений по новым событиям + сигнал UI.
pub async fn dispatch_events(app: &AppHandle, ctx: crate::state::Ctx, ids: Vec<i64>) {
    let _ = app.emit("rid://events", &ids);
    let list = notifications::pending(&ctx, &ids).await;
    for (e, title, body) in list {
        if ctx.settings.notifications.windows {
            use tauri_plugin_notification::NotificationExt;
            if let Err(err) = app.notification().builder().title(&title).body(&body).show() {
                tracing::warn!(error = %err, "уведомление Windows");
            }
        }
        if ctx.settings.notifications.telegram
            && let Some(chat) = &ctx.settings.notifications.telegram_chat_id
            && let Err(err) = notifications::send_telegram(&ctx, chat, &format!("RID · {title}\n{body}")).await
        {
            tracing::warn!(error = %err, "Telegram");
            let _ = repo::record_health_error(&ctx.pool, "telegram", "degraded", err.kind(), &err.to_string()).await;
        }
        let _ = repo::mark_event_notified(&ctx.pool, e.id).await;
    }
}

// -------------------------------------------------------------------- bans

#[derive(Serialize)]
pub struct BanOverviewRow {
    pub player: PlayerRow,
    pub bans: Vec<BanRow>,
    pub risk: RiskSummary,
}

#[tauri::command]
pub async fn get_bans_overview(state: St<'_>) -> AppResult<Vec<BanOverviewRow>> {
    build_bans_overview(&state.ctx().await?).await
}

pub async fn build_bans_overview(c: &crate::state::Ctx) -> AppResult<Vec<BanOverviewRow>> {
    let all = repo::list_all_bans(&c.pool).await?;
    let mut out = vec![];
    for p in repo::list_players(&c.pool, None, false, 1000, 0).await? {
        let bans: Vec<BanRow> = all.iter().filter(|b| b.steam_id == p.steam_id).cloned().collect();
        let risk = player_service::risk_for(c, &p, &bans);
        out.push(BanOverviewRow { player: p, bans, risk });
    }
    out.sort_by(|a, b| b.risk.score.partial_cmp(&a.risk.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

// ----------------------------------------------------------------- servers

#[tauri::command]
pub async fn list_servers(state: St<'_>, query: Option<String>, followed_only: Option<bool>) -> AppResult<Vec<ServerRow>> {
    Ok(repo::list_servers(&state.ctx().await?.pool, query.as_deref(), followed_only.unwrap_or(false), 500).await?)
}

#[tauri::command]
pub async fn refresh_servers(state: St<'_>, query: Option<String>) -> AppResult<RefreshResult> {
    server_service::refresh(&state.ctx().await?, query.as_deref()).await
}

#[tauri::command]
pub async fn set_server_followed(state: St<'_>, id: String, followed: bool) -> AppResult<()> {
    Ok(repo::set_server_followed(&state.ctx().await?.pool, &id, followed).await?)
}

#[tauri::command]
pub async fn get_server_population(state: St<'_>, id: String, period: String) -> AppResult<PopulationReport> {
    server_service::population(&state.ctx().await?, &id, &period).await
}

// ------------------------------------------------------------ integrations

#[derive(Serialize)]
pub struct IntegrationStatus {
    pub descriptor: ProviderDescriptor,
    pub configured: bool,
    pub masked: Option<String>,
    pub health: Option<HealthRow>,
    pub access: AccessStatus,
}

fn key_for(credential: &str) -> Option<SecretKey> {
    SecretKey::ALL.into_iter().find(|k| k.account() == credential)
}

#[tauri::command]
pub async fn get_integrations(state: St<'_>) -> AppResult<Vec<IntegrationStatus>> {
    build_integrations(&state.ctx().await?).await
}

pub async fn build_integrations(c: &crate::state::Ctx) -> AppResult<Vec<IntegrationStatus>> {
    let health = repo::list_health(&c.pool).await?;
    let mut out = vec![];
    for d in descriptors() {
        let key = d.credential.and_then(key_for);
        let secret = key.and_then(|k| c.secrets.get(k));
        let h = health.iter().find(|h| h.provider == d.id).cloned();
        let access = match (&h, d.id) {
            (_, "project_bans") => AccessStatus::Unsupported,
            (Some(h), _) if h.last_success_at.is_some() => AccessStatus::Verified,
            (Some(h), _) if h.last_error_kind.as_deref() == Some("forbidden") => AccessStatus::Denied,
            _ => AccessStatus::NotTested,
        };
        out.push(IntegrationStatus { configured: secret.is_some(), masked: secret.as_deref().map(mask), health: h, access, descriptor: d });
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_secret(state: St<'_>, key: SecretKey, value: String) -> AppResult<SecretBackend> {
    let backend = state.secrets.set(key, &value).map_err(AppError::validation)?;
    state.tracker_wake.notify_one();
    Ok(backend)
}

#[tauri::command]
pub async fn clear_secret(state: St<'_>, key: SecretKey) -> AppResult<()> {
    state.secrets.delete(key);
    Ok(())
}

#[tauri::command]
pub async fn test_integration(state: St<'_>, provider: String) -> AppResult<String> {
    let c = state.ctx().await?;
    let r = match provider.as_str() {
        "steam" => c.steam.test().await,
        "battlemetrics" => c.bm.test().await,
        "battlemetrics_ru" => c.bmru.test().await,
        "telegram" => {
            let chat = c.settings.notifications.telegram_chat_id.clone().ok_or_else(|| AppError::validation("Укажите chat ID в настройках уведомлений"))?;
            notifications::send_telegram(&c, &chat, "RID: тестовое сообщение. Уведомления работают, пока приложение запущено.")
                .await
                .map(|_| "Сообщение отправлено".to_string())
        }
        _ => return Err(AppError::validation("Неизвестный провайдер")),
    };
    match r {
        Ok(m) => {
            repo::record_health_ok(&c.pool, &provider).await?;
            Ok(m)
        }
        Err(e) => {
            player_service::record_err(&c, &provider, &e).await;
            Err(AppError::provider(&provider, e))
        }
    }
}

#[tauri::command]
pub async fn send_test_notification(app: AppHandle) -> AppResult<()> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title("Rust Intelligence Desktop")
        .body("Тестовое уведомление")
        .show()
        .map_err(|e| AppError::Internal { message: e.to_string() })
}

// -------------------------------------------------------------------- data

fn check_path(p: &str, exts: &[&str]) -> AppResult<PathBuf> {
    let path = PathBuf::from(p);
    if !path.is_absolute() {
        return Err(AppError::validation("Нужен абсолютный путь"));
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !exts.contains(&ext.as_str()) {
        return Err(AppError::validation(format!("Ожидается файл: {}", exts.join(", "))));
    }
    Ok(path)
}

#[tauri::command]
pub async fn export_data(state: St<'_>, path: String, format: String) -> AppResult<usize> {
    let ext: &[&str] = if format == "json" { &["json"] } else { &["csv"] };
    export::export(&state.ctx().await?, &check_path(&path, ext)?, &format).await
}

#[tauri::command]
pub async fn import_data(state: St<'_>, path: String) -> AppResult<ImportResult> {
    export::import(&state.ctx().await?, &check_path(&path, &["json"])?).await
}

#[tauri::command]
pub async fn backup_database(state: St<'_>, path: String) -> AppResult<String> {
    let p = check_path(&path, &["sqlite", "db"])?;
    db::backup_to(&state.ctx().await?.pool, &p).await?;
    Ok(p.display().to_string())
}

#[tauri::command]
pub async fn restore_database(state: St<'_>, path: String) -> AppResult<()> {
    let p = check_path(&path, &["sqlite", "db"])?;
    let mode = state.ctx().await?.mode;
    state.replace_database(mode, &p).await
}

#[derive(Serialize)]
pub struct Diagnostics {
    pub integrity: String,
    pub schema_version: i64,
    pub db_path: String,
    pub db_size_bytes: u64,
    pub counts: Vec<(String, i64)>,
    pub health: Vec<HealthRow>,
    pub tracker: TrackerStatus,
    pub secret_backend: SecretBackend,
    pub generated_at: String,
}

#[tauri::command]
pub async fn get_diagnostics(state: St<'_>) -> AppResult<Diagnostics> {
    let c = state.ctx().await?;
    let path = state.db_path(c.mode);
    Ok(Diagnostics {
        integrity: db::integrity_check(&c.pool).await?,
        schema_version: db::schema_version(&c.pool).await,
        db_size_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
        db_path: path.display().to_string(),
        counts: repo::table_counts(&c.pool).await?,
        health: repo::list_health(&c.pool).await?,
        tracker: tracker_service::STATUS.read().await.clone(),
        secret_backend: state.secrets.backend(),
        generated_at: fmt_ts(now()),
    })
}

#[tauri::command]
pub async fn purge_all_data(state: St<'_>, confirm: String) -> AppResult<()> {
    if confirm != "УДАЛИТЬ" {
        return Err(AppError::validation("Для подтверждения введите УДАЛИТЬ"));
    }
    let c = state.ctx().await?;
    repo::purge_all(&c.pool).await?;
    if c.is_demo() {
        // демо пересоздаётся при следующем открытии
        state.drop_database(Mode::Demo).await?;
        state.set_mode(Mode::Demo).await?;
    }
    Ok(())
}

// --------------------------------------------------------------- dashboard

#[derive(Serialize)]
pub struct DailyCount {
    pub date: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct Dashboard {
    pub mode: Mode,
    pub players_total: i64,
    pub favorites: Vec<PlayerRow>,
    pub tracked: Vec<TrackedRow>,
    pub recent_events: Vec<EventRow>,
    pub unread_events: i64,
    pub health: Vec<HealthRow>,
    pub events_per_day: Vec<DailyCount>,
    pub squads: Vec<SquadRow>,
    pub followed_servers: Vec<ServerRow>,
    pub tracker: TrackerStatus,
}

#[tauri::command]
pub async fn get_dashboard(state: St<'_>) -> AppResult<Dashboard> {
    build_dashboard(&state.ctx().await?).await
}

pub async fn build_dashboard(c: &crate::state::Ctx) -> AppResult<Dashboard> {
    let since = fmt_ts(now() - chrono::Duration::days(14));
    let per_day: Vec<(String, i64)> = sqlx::query_as(
        "SELECT substr(occurred_upper, 1, 10) AS d, COUNT(*) FROM events WHERE occurred_upper >= ? AND steam_id IS NOT NULL GROUP BY d ORDER BY d",
    )
    .bind(&since)
    .fetch_all(&c.pool)
    .await?;
    Ok(Dashboard {
        mode: c.mode,
        players_total: repo::count_players(&c.pool).await?,
        favorites: repo::list_players(&c.pool, None, true, 12, 0).await?,
        tracked: repo::list_tracked(&c.pool).await?,
        recent_events: repo::list_events(&c.pool, None, None, false, 12, 0).await?,
        unread_events: repo::count_unread_events(&c.pool).await?,
        health: repo::list_health(&c.pool).await?,
        events_per_day: per_day.into_iter().map(|(date, count)| DailyCount { date, count }).collect(),
        squads: repo::list_squads(&c.pool).await?,
        followed_servers: repo::list_servers(&c.pool, None, true, 8).await?,
        tracker: tracker_service::STATUS.read().await.clone(),
    })
}
