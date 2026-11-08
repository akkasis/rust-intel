//! Планировщик трекера: работает, пока запущено приложение (включая трей).
//! Сбой источника → состояние unknown и событие source_unavailable, но никогда не «вышел».

use crate::domain::ids::SteamId64;
use crate::domain::tracking::EventKind;
use crate::infrastructure::clock::{fmt_ts, now, parse_ts};
use crate::persistence::repo::{self, NewEvent, NewSession};
use crate::providers::battlemetrics::BmPresence;
use crate::providers::demo::DemoPresence;
use crate::providers::{Presence, PresenceSource, PresenceState};
use crate::state::{AppState, Ctx};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct NewEventNote {
    pub id: i64,
    pub steam_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
}

/// Результат обработки одного наблюдения.
pub async fn process_presence(ctx: &Ctx, provider: &str, p: &Presence, t: DateTime<Utc>, interval_secs: i64) -> Result<Vec<i64>, sqlx::Error> {
    let sid = p.steam_id.as_str();
    let pool = &ctx.pool;
    let mut new_events = Vec::new();
    let ever = repo::has_observations(pool, sid).await?;
    let prev = repo::last_observation(pool, sid, provider).await?;
    repo::insert_observation(pool, sid, provider, t, p.state.as_str(), p.server_id.as_deref(), interval_secs).await?;
    let max_gap = Duration::seconds((interval_secs as f64 * 2.5) as i64);
    let prev_t = prev.as_ref().and_then(|o| parse_ts(&o.observed_at));
    let contiguous = prev.as_ref().is_some_and(|o| o.state != "unknown") && prev_t.is_some_and(|pt| t - pt <= max_gap && t > pt);
    if contiguous && p.state != PresenceState::Unknown {
        repo::add_coverage(pool, sid, provider, prev_t.unwrap_or(t), t).await?;
    }
    let lower = if contiguous { prev_t.unwrap_or(t) } else { t };
    let open = repo::open_session(pool, sid, provider).await?;
    let ev = |kind: EventKind, server: Option<String>, details: serde_json::Value| mk_event(provider, sid, kind, server, lower, t, details);
    let steam_scope = provider == "steam";
    match p.state {
        PresenceState::Online => {
            let same = open.as_ref().is_some_and(|o| contiguous && o.server_id == p.server_id);
            if same {
                let o = open.as_ref().unwrap();
                repo::upsert_session(
                    pool,
                    &NewSession {
                        steam_id: sid,
                        provider,
                        kind: "inferred_from_polls",
                        server_id: o.server_id.as_deref(),
                        start_lower: parse_ts(&o.start_lower).unwrap_or(t),
                        start_upper: parse_ts(&o.start_upper).unwrap_or(t),
                        end_lower: t,
                        end_upper: None,
                        is_open: true,
                        external_id: None,
                    },
                )
                .await?;
            } else {
                let switched = open.as_ref().is_some_and(|o| contiguous && o.server_id != p.server_id);
                if let Some(o) = &open {
                    close_session(ctx, provider, sid, o, if contiguous { Some(t) } else { None }).await?;
                }
                repo::upsert_session(
                    pool,
                    &NewSession {
                        steam_id: sid,
                        provider,
                        kind: "inferred_from_polls",
                        server_id: p.server_id.as_deref(),
                        start_lower: lower,
                        start_upper: t,
                        end_lower: t,
                        end_upper: None,
                        is_open: true,
                        external_id: None,
                    },
                )
                .await?;
                // События только для реально наблюдённых переходов (без пробелов в наблюдении).
                if contiguous {
                    let kind = if switched {
                        EventKind::SwitchedObservedServer
                    } else if steam_scope {
                        EventKind::SteamInGameStarted
                    } else {
                        EventKind::JoinedObservedServer
                    };
                    let d = serde_json::json!({"poll_interval_secs": interval_secs, "server_name": p.server_name});
                    if let Some(id) = repo::insert_event(pool, &ev(kind, p.server_id.clone(), d).as_new()).await? {
                        new_events.push(id);
                    }
                }
            }
        }
        PresenceState::Offline => {
            if let Some(o) = &open {
                close_session(ctx, provider, sid, o, if contiguous { Some(t) } else { None }).await?;
                if contiguous {
                    let kind = if steam_scope { EventKind::SteamInGameStopped } else { EventKind::LeftObservedServer };
                    let d = serde_json::json!({"poll_interval_secs": interval_secs});
                    if let Some(id) = repo::insert_event(pool, &ev(kind, o.server_id.clone(), d).as_new()).await? {
                        new_events.push(id);
                    }
                }
            }
        }
        PresenceState::Unknown => {
            if let Some(o) = &open {
                close_session(ctx, provider, sid, o, None).await?;
            }
        }
    }
    if !ever && p.state != PresenceState::Unknown {
        let d = serde_json::json!({"state": p.state.as_str()});
        let mut e = ev(EventKind::FirstSeen, p.server_id.clone(), d);
        e.approximate = false;
        e.occurred_lower = t;
        if let Some(id) = repo::insert_event(pool, &e.as_new()).await? {
            new_events.push(id);
        }
    }
    if let Some(name) = p.persona_name.as_deref().filter(|n| !n.is_empty()) {
        let player = repo::get_player(pool, sid).await?;
        let old = player.as_ref().and_then(|x| x.persona_name.clone());
        repo::record_alias(pool, sid, name, provider, t).await?;
        if let Some(old) = old.filter(|o| o != name) {
            repo::upsert_player(
                pool,
                &repo::PlayerUpsert { steam_id: sid.into(), persona_name: Some(name.into()), is_demo: ctx.is_demo(), ..Default::default() },
            )
            .await?;
            let mut e = ev(EventKind::NameChanged, None, serde_json::json!({"from": old, "to": name}));
            e.dedup_key = format!("{provider}:name:{sid}:{name}");
            if let Some(id) = repo::insert_event(pool, &e.as_new()).await? {
                new_events.push(id);
            }
        }
    }
    repo::update_tracked_state(pool, sid, p.state.as_str(), p.server_id.as_deref(), t, p.state != PresenceState::Unknown).await?;
    if steam_scope && p.state != PresenceState::Unknown {
        repo::upsert_player(
            pool,
            &repo::PlayerUpsert { steam_id: sid.into(), steam_in_game: Some(p.state == PresenceState::Online), is_demo: ctx.is_demo(), ..Default::default() },
        )
        .await?;
    }
    Ok(new_events)
}

struct OwnedEvent {
    steam_id: String,
    provider: String,
    kind: EventKind,
    dedup_key: String,
    occurred_lower: DateTime<Utc>,
    occurred_upper: DateTime<Utc>,
    approximate: bool,
    server_id: Option<String>,
    details: serde_json::Value,
}

impl OwnedEvent {
    fn as_new(&self) -> NewEvent<'_> {
        NewEvent {
            steam_id: Some(&self.steam_id),
            provider: &self.provider,
            kind: self.kind.as_str(),
            dedup_key: self.dedup_key.clone(),
            occurred_lower: self.occurred_lower,
            occurred_upper: self.occurred_upper,
            approximate: self.approximate,
            server_id: self.server_id.as_deref(),
            details: self.details.clone(),
        }
    }
}

fn mk_event(
    provider: &str,
    sid: &str,
    kind: EventKind,
    server: Option<String>,
    lower: DateTime<Utc>,
    t: DateTime<Utc>,
    details: serde_json::Value,
) -> OwnedEvent {
    OwnedEvent {
        steam_id: sid.into(),
        provider: provider.into(),
        kind,
        dedup_key: format!("{provider}:{}:{sid}:{}", kind.as_str(), fmt_ts(t)),
        occurred_lower: lower,
        occurred_upper: t,
        approximate: true,
        server_id: server,
        details,
    }
}

async fn close_session(
    ctx: &Ctx,
    provider: &str,
    sid: &str,
    o: &crate::persistence::models::SessionRow,
    end_upper: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    repo::upsert_session(
        &ctx.pool,
        &NewSession {
            steam_id: sid,
            provider,
            kind: "inferred_from_polls",
            server_id: o.server_id.as_deref(),
            start_lower: parse_ts(&o.start_lower).unwrap_or_else(now),
            start_upper: parse_ts(&o.start_upper).unwrap_or_else(now),
            end_lower: parse_ts(&o.end_lower).unwrap_or_else(now),
            end_upper,
            is_open: false,
            external_id: None,
        },
    )
    .await
}

async fn sources(ctx: &Ctx) -> Vec<Arc<dyn PresenceSource>> {
    if ctx.is_demo() {
        return vec![Arc::new(DemoPresence)];
    }
    let mut v: Vec<Arc<dyn PresenceSource>> = Vec::new();
    if ctx.steam.is_configured() {
        v.push(ctx.steam.clone());
    }
    if ctx.bm.is_configured() {
        let mut links = HashMap::new();
        for t in repo::list_tracked(&ctx.pool).await.unwrap_or_default() {
            for l in repo::list_links(&ctx.pool, &t.steam_id).await.unwrap_or_default() {
                if l.provider == "battlemetrics"
                    && let Ok(id) = SteamId64::parse(&t.steam_id)
                {
                    links.insert(id, crate::domain::ids::BattleMetricsPlayerId(l.external_id));
                }
            }
        }
        if !links.is_empty() {
            v.push(Arc::new(BmPresence { provider: ctx.bm.clone(), links }));
        }
    }
    v
}

/// Один цикл опроса. Возвращает id новых событий.
pub async fn poll_once(ctx: &Ctx) -> Result<Vec<i64>, sqlx::Error> {
    let tracked: Vec<SteamId64> =
        repo::list_tracked(&ctx.pool).await?.into_iter().filter(|t| t.enabled).filter_map(|t| SteamId64::parse(&t.steam_id).ok()).collect();
    if tracked.is_empty() {
        return Ok(vec![]);
    }
    let mut new_events = Vec::new();
    let interval = ctx.settings.poll_interval_secs as i64;
    for src in sources(ctx).await {
        let id = src.id();
        let t = now();
        let interval = interval.max(src.min_poll_secs() as i64);
        let prev_health = repo::get_health(&ctx.pool, id).await?;
        match src.presence(&tracked).await {
            Ok(list) => {
                repo::record_health_ok(&ctx.pool, id).await?;
                if prev_health.as_ref().is_some_and(|h| h.status != "ok") {
                    let e = source_event(id, EventKind::SourceRestored, t, serde_json::json!({"provider": id}));
                    if let Some(x) = repo::insert_event(&ctx.pool, &e).await? {
                        new_events.push(x);
                    }
                }
                for p in &list {
                    new_events.extend(process_presence(ctx, id, p, t, interval).await?);
                    if let Some(addr) = &p.game_server_ip {
                        auto_link_from_server(ctx, &p.steam_id, addr, t).await;
                    }
                }
            }
            Err(err) => {
                tracing::warn!(provider = id, error = %err, "источник недоступен");
                crate::services::player_service::record_err(ctx, id, &err).await;
                if prev_health.as_ref().is_none_or(|h| h.status == "ok") {
                    let e =
                        source_event(id, EventKind::SourceUnavailable, t, serde_json::json!({"provider": id, "error": err.to_string(), "kind": err.kind()}));
                    if let Some(x) = repo::insert_event(&ctx.pool, &e).await? {
                        new_events.push(x);
                    }
                }
                // Состояние игроков — неизвестно (не офлайн).
                for sid in &tracked {
                    let p = Presence {
                        steam_id: sid.clone(),
                        game_server_ip: None,
                        state: PresenceState::Unknown,
                        server_id: None,
                        server_name: None,
                        persona_name: None,
                    };
                    process_presence(ctx, id, &p, t, interval).await?;
                }
            }
        }
    }
    Ok(new_events)
}

/// Игрок в Rust и Steam сообщил сервер → запомнить и, если BM ещё не привязан, попробовать привязать.
async fn auto_link_from_server(ctx: &Ctx, sid: &SteamId64, addr: &str, t: DateTime<Utc>) {
    let _ = sqlx::query("UPDATE players SET last_game_server = ?, last_game_server_at = ? WHERE steam_id = ?")
        .bind(addr)
        .bind(fmt_ts(t))
        .bind(sid.as_str())
        .execute(&ctx.pool)
        .await;
    if ctx.is_demo() || !ctx.bm.is_configured() || crate::services::linking::is_linked(ctx, sid.as_str()).await.unwrap_or(true) {
        return;
    }
    let mut notices = vec![];
    if let Ok(Some(bm)) = crate::services::linking::try_server_ip_link(ctx, sid.as_str(), addr, &mut notices).await
        && crate::services::player_service::save_link(ctx, sid.as_str(), &bm, "server_ip_match").await.is_ok()
    {
        let _ = crate::services::player_service::sync_bm(ctx, sid, &bm, &mut notices).await;
        tracing::info!(steam = sid.as_str(), bm = bm.0, "BattleMetrics привязан по серверу из Steam");
    }
}

fn source_event(provider: &'static str, kind: EventKind, t: DateTime<Utc>, details: serde_json::Value) -> NewEvent<'static> {
    NewEvent {
        steam_id: None,
        provider,
        kind: kind.as_str(),
        dedup_key: format!("{provider}:{}:{}", kind.as_str(), fmt_ts(t)),
        occurred_lower: t,
        occurred_upper: t,
        approximate: false,
        server_id: None,
        details,
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TrackerStatus {
    pub running: bool,
    pub paused: bool,
    pub last_cycle_at: Option<String>,
    pub next_cycle_at: Option<String>,
    pub interval_secs: u64,
    pub sources: Vec<String>,
    pub note: String,
}

pub static STATUS: std::sync::LazyLock<tokio::sync::RwLock<TrackerStatus>> = std::sync::LazyLock::new(Default::default);

/// Фоновый цикл. `on_events` вызывается с новыми событиями (уведомления/обновление UI).
pub async fn run_loop<F>(state: Arc<AppState>, on_events: F)
where
    F: Fn(Ctx, Vec<i64>) + Send + Sync + 'static,
{
    let mut last_prune = now() - Duration::days(1);
    loop {
        let ctx = state.ctx().await;
        let mut wait = std::time::Duration::from_secs(60);
        if let Ok(ctx) = ctx {
            let interval = ctx.settings.poll_interval_secs.max(60);
            wait = std::time::Duration::from_secs(if ctx.is_demo() { 60 } else { interval });
            let paused = ctx.settings.tracking_paused;
            let srcs: Vec<String> = sources(&ctx).await.iter().map(|s| s.id().to_string()).collect();
            if !paused {
                match poll_once(&ctx).await {
                    Ok(ev) if !ev.is_empty() => on_events(ctx.clone(), ev),
                    Ok(_) => {}
                    Err(e) => tracing::error!(error = %e, "ошибка цикла трекера"),
                }
                match crate::services::bm_service::poll_watched(&ctx).await {
                    Ok(ev) if !ev.is_empty() => on_events(ctx.clone(), ev),
                    Ok(_) => {}
                    Err(e) => tracing::error!(error = %e, "ошибка наблюдения за серверами"),
                }
            }
            if now() - last_prune > Duration::hours(6) {
                let cutoff = fmt_ts(now() - Duration::days(ctx.settings.retention_days));
                if let Ok(n) = repo::prune_older_than(&ctx.pool, &cutoff).await {
                    tracing::info!(rows = n, "очистка по сроку хранения");
                }
                last_prune = now();
            }
            let t = now();
            *STATUS.write().await = TrackerStatus {
                running: true,
                paused,
                last_cycle_at: (!paused).then(|| fmt_ts(t)),
                next_cycle_at: Some(fmt_ts(t + Duration::seconds(wait.as_secs() as i64))),
                interval_secs: wait.as_secs(),
                note: if srcs.is_empty() {
                    "Нет настроенных источников присутствия: добавьте Steam Web API key".into()
                } else {
                    "Опрос идёт, только пока приложение запущено (в т.ч. в трее). Выключенный или спящий ПК не наблюдает.".into()
                },
                sources: srcs,
            };
        }
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = state.tracker_wake.notified() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::player_service::tests::demo_ctx;

    fn pres(id: &SteamId64, s: PresenceState, server: Option<&str>) -> Presence {
        Presence { steam_id: id.clone(), game_server_ip: None, state: s, server_id: server.map(str::to_string), server_name: None, persona_name: None }
    }

    #[tokio::test]
    async fn poll_transitions_events_and_dedup() {
        let mut ctx = demo_ctx().await;
        ctx.pool = crate::persistence::db::open_memory().await.unwrap();
        let id = SteamId64::parse("76561197960287930").unwrap();
        repo::ensure_player(&ctx.pool, id.as_str(), false).await.unwrap();
        let t0 = parse_ts("2026-11-01T10:00:00Z").unwrap();
        let step = Duration::seconds(120);
        let e1 = process_presence(&ctx, "battlemetrics", &pres(&id, PresenceState::Offline, None), t0, 120).await.unwrap();
        assert_eq!(e1.len(), 1, "first_seen");
        let e2 = process_presence(&ctx, "battlemetrics", &pres(&id, PresenceState::Online, Some("s1")), t0 + step, 120).await.unwrap();
        assert_eq!(e2.len(), 1, "joined");
        // повтор того же сэмпла не создаёт дублей
        let e2b = process_presence(&ctx, "battlemetrics", &pres(&id, PresenceState::Online, Some("s1")), t0 + step, 120).await.unwrap();
        assert!(e2b.is_empty());
        let e3 = process_presence(&ctx, "battlemetrics", &pres(&id, PresenceState::Online, Some("s2")), t0 + step * 2, 120).await.unwrap();
        assert_eq!(e3.len(), 1, "switched");
        // сбой источника: не «вышел»
        let e4 = process_presence(&ctx, "battlemetrics", &pres(&id, PresenceState::Unknown, None), t0 + step * 3, 120).await.unwrap();
        assert!(e4.is_empty());
        // после сбоя — офлайн: перехода не наблюдали, события «вышел» нет
        let e5 = process_presence(&ctx, "battlemetrics", &pres(&id, PresenceState::Offline, None), t0 + step * 4, 120).await.unwrap();
        assert!(e5.is_empty());
        let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM events ORDER BY id").fetch_all(&ctx.pool).await.unwrap();
        assert_eq!(kinds, vec!["first_seen", "joined_observed_server", "switched_observed_server"]);
        let s = repo::list_sessions(&ctx.pool, id.as_str(), "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await.unwrap();
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| !x.is_open));
        let s2 = s.iter().find(|x| x.server_id.as_deref() == Some("s2")).unwrap();
        assert!(s2.end_upper.is_none(), "конец сессии неизвестен из-за сбоя");
    }

    #[tokio::test]
    async fn long_gap_breaks_session_without_events() {
        let mut ctx = demo_ctx().await;
        ctx.pool = crate::persistence::db::open_memory().await.unwrap();
        let id = SteamId64::parse("76561197960287930").unwrap();
        repo::ensure_player(&ctx.pool, id.as_str(), false).await.unwrap();
        let t0 = parse_ts("2026-11-01T10:00:00Z").unwrap();
        process_presence(&ctx, "steam", &pres(&id, PresenceState::Online, None), t0, 120).await.unwrap();
        // ПК спал 3 часа
        let ev = process_presence(&ctx, "steam", &pres(&id, PresenceState::Offline, None), t0 + Duration::hours(3), 120).await.unwrap();
        assert!(ev.is_empty(), "без наблюдаемого перехода нет события");
        let cov = repo::list_coverage(&ctx.pool, id.as_str(), "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await.unwrap();
        assert!(cov.is_empty(), "пробел не считается покрытием");
    }
}
