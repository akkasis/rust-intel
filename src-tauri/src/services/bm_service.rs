//! BattleMetrics как основной источник: игроки BM, наблюдение за серверами, тиммейты.

use super::RangeSpec;
use crate::domain::analytics::{ActivityAggregate, HeatmapOptions, Interval, PlayerTimeline, SessionBounds, aggregate_activity};
use crate::domain::ids::{BattleMetricsPlayerId, SteamId64};
use crate::domain::teammates::{ObservedSession, TeammateCandidate, TeammateOptions, find_teammates};
use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::{fmt_ts, now, parse_ts};
use crate::persistence::bm_repo::{self, BmNameRow, BmPlayerRow, BmServerRow, BmSessionRow, NewBmSession, WatchRow};
use crate::persistence::models::{PlayerRow, ServerRow};
use crate::persistence::repo::{self, NewEvent};
use crate::providers::battlemetrics::{BmServerStat, parse_player_id};
use crate::services::player_service::{self, Notice};
use crate::state::Ctx;
use chrono::Duration;
use serde::Serialize;
use std::collections::HashMap;

pub fn parse_bm_id(input: &str) -> AppResult<BattleMetricsPlayerId> {
    parse_player_id(input).ok_or_else(|| AppError::validation("Нужен ID игрока BattleMetrics или ссылка вида battlemetrics.com/players/123456"))
}

fn server_row_from(s: &crate::providers::ServerInfo, existing: Option<&ServerRow>) -> ServerRow {
    ServerRow {
        id: format!("{}:{}", s.provider, s.external_id),
        provider: s.provider.into(),
        external_id: s.external_id.clone(),
        name: s.name.clone(),
        address: s.address.clone(),
        country: s.country.clone(),
        map: s.map.clone().or_else(|| existing.and_then(|e| e.map.clone())),
        players: s.players,
        max_players: s.max_players,
        queue: s.queue,
        online: s.online,
        trust: s.trust.clone(),
        last_wipe_at: s.last_wipe_at.map(fmt_ts),
        next_wipe_at: existing.and_then(|e| e.next_wipe_at.clone()),
        followed: existing.is_some_and(|e| e.followed),
        source_url: s.source_url.clone(),
        fetched_at: fmt_ts(now()),
        is_demo: false,
    }
}

async fn bm_event(
    ctx: &Ctx,
    kind: &str,
    bm_id: &str,
    name: &str,
    server_id: &str,
    lower: chrono::DateTime<chrono::Utc>,
    t: chrono::DateTime<chrono::Utc>,
) -> AppResult<Option<i64>> {
    let p = bm_repo::get_player(&ctx.pool, bm_id).await?;
    let steam = match p.and_then(|p| p.steam_id) {
        Some(s) if repo::get_player(&ctx.pool, &s).await?.is_some() => Some(s),
        _ => None,
    };
    Ok(repo::insert_event(
        &ctx.pool,
        &NewEvent {
            steam_id: steam.as_deref(),
            provider: "battlemetrics",
            kind,
            dedup_key: format!("bm:{kind}:{bm_id}:{server_id}:{}", fmt_ts(t)),
            occurred_lower: lower,
            occurred_upper: t,
            approximate: true,
            server_id: Some(server_id),
            details: serde_json::json!({"bm_id": bm_id, "name": name}),
        },
    )
    .await?)
}

/// Один цикл наблюдения за отмеченными серверами. Возвращает id новых событий (по отслеживаемым игрокам BM).
pub async fn poll_watched(ctx: &Ctx) -> AppResult<Vec<i64>> {
    if ctx.is_demo() || !ctx.bm.is_configured() {
        return Ok(vec![]);
    }
    let interval = ctx.settings.poll_interval_secs.max(60) as i64;
    let mut out = vec![];
    for w in bm_repo::list_watch(&ctx.pool).await?.into_iter().filter(|w| w.enabled) {
        let Some(ext) = w.server_id.strip_prefix("battlemetrics:") else { continue };
        let t = now();
        match ctx.bm.server_roster(ext).await {
            Ok(r) => {
                repo::record_health_ok(&ctx.pool, "battlemetrics").await?;
                let existing = repo::get_server(&ctx.pool, &w.server_id).await?;
                repo::upsert_server(&ctx.pool, &server_row_from(&r.server, existing.as_ref())).await?;
                let delta = bm_repo::apply_roster(&ctx.pool, &w.server_id, Some(&r.players), t, interval, false).await?;
                let lower = t - Duration::seconds(interval);
                for (kind, list) in [("joined_observed_server", &delta.joined), ("left_observed_server", &delta.left)] {
                    for (id, name) in list {
                        if bm_repo::get_player(&ctx.pool, id).await?.is_some_and(|p| p.tracked)
                            && let Some(e) = bm_event(ctx, kind, id, name, &w.server_id, lower, t).await?
                        {
                            out.push(e);
                        }
                    }
                }
            }
            Err(e) => {
                player_service::record_err(ctx, "battlemetrics", &e).await;
                bm_repo::apply_roster(&ctx.pool, &w.server_id, None, t, interval, false).await?;
                bm_repo::record_watch_error(&ctx.pool, &w.server_id, &e.to_string()).await?;
            }
        }
    }
    out.extend(crate::services::insights_service::check_clan_gatherings(ctx).await?);
    Ok(out)
}

// ------------------------------------------------------------------ player view

#[derive(Debug, Serialize)]
pub struct TeammateView {
    #[serde(flatten)]
    pub candidate: TeammateCandidate,
    pub name: Option<String>,
    pub steam_id: Option<String>,
    pub tracked: bool,
}

#[derive(Debug, Serialize)]
pub struct BmPlayerView {
    pub player: BmPlayerRow,
    pub source_url: String,
    pub names: Vec<BmNameRow>,
    pub servers: Vec<BmServerRow>,
    pub sessions: Vec<BmSessionRow>,
    pub sessions_total: i64,
    pub observed_secs: i64,
    pub online_now: Option<BmServerRow>,
    pub steam: Option<PlayerRow>,
    pub teammates: Vec<TeammateView>,
    pub teammates_note: String,
    pub notices: Vec<Notice>,
    pub watched_servers: usize,
}

/// Загружает из BM профиль игрока (серверы, ники) и историю сессий, если доступна.
pub async fn refresh_player(ctx: &Ctx, bm: &BattleMetricsPlayerId, notices: &mut Vec<Notice>) -> AppResult<()> {
    let t = now();
    let info = ctx.bm.player_info(bm).await.map_err(|e| AppError::provider("battlemetrics", e))?;
    repo::record_health_ok(&ctx.pool, "battlemetrics").await?;
    let last = info.servers.iter().filter_map(|s| s.last_seen).max();
    bm_repo::upsert_player(&ctx.pool, &bm.0, info.name.as_deref(), last, false).await?;
    for n in &info.names {
        bm_repo::record_name(&ctx.pool, &bm.0, n, &fmt_ts(last.unwrap_or(t))).await?;
    }
    if let Some(steam) = &info.steam_id
        && SteamId64::parse(steam).is_ok()
    {
        bm_repo::set_steam(&ctx.pool, &bm.0, Some(steam)).await?;
    }
    for st in &info.servers {
        upsert_bm_server(ctx, st).await?;
        bm_repo::upsert_player_server(&ctx.pool, &bm.0, st, t).await?;
    }
    bm_repo::set_fetched(&ctx.pool, &bm.0, t).await?;
    match ctx.bm.sessions(bm).await {
        Ok(list) => {
            for s in &list {
                let Some(server) = &s.server_id else { continue };
                bm_repo::upsert_session(
                    &ctx.pool,
                    &NewBmSession {
                        bm_id: &bm.0,
                        server_id: server,
                        source: "bm_api",
                        start_lower: s.start,
                        start_upper: s.start,
                        start_known: true,
                        end_lower: s.stop.unwrap_or(t),
                        end_upper: s.stop,
                        end_known: s.stop.is_some(),
                        is_open: s.stop.is_none(),
                        external_id: Some(&s.external_id),
                    },
                )
                .await?;
            }
            notices.push(Notice {
                level: "info", text: format!("BattleMetrics: серверов {}, сессий из истории {}", info.servers.len(), list.len())
            });
        }
        Err(e) => notices.push(Notice {
            level: "warning",
            text: format!(
                "BattleMetrics не отдал историю сессий ({e}). Серверы и время загружены; точные входы/выходы будут копиться при наблюдении за серверами."
            ),
        }),
    }
    Ok(())
}

async fn upsert_bm_server(ctx: &Ctx, st: &BmServerStat) -> AppResult<()> {
    let existing = repo::get_server(&ctx.pool, &st.server_id).await?;
    let ext = st.server_id.trim_start_matches("battlemetrics:");
    repo::upsert_server(
        &ctx.pool,
        &ServerRow {
            id: st.server_id.clone(),
            provider: "battlemetrics".into(),
            external_id: ext.into(),
            name: st.server_name.clone(),
            address: st.address.clone(),
            country: st.country.clone(),
            map: existing.as_ref().and_then(|e| e.map.clone()),
            players: st.players,
            max_players: st.max_players,
            queue: None,
            online: existing.as_ref().and_then(|e| e.online),
            trust: None,
            last_wipe_at: None,
            next_wipe_at: None,
            followed: existing.as_ref().is_some_and(|e| e.followed),
            source_url: Some(format!("https://www.battlemetrics.com/servers/rust/{ext}")),
            fetched_at: fmt_ts(now()),
            is_demo: false,
        },
    )
    .await?;
    Ok(())
}

fn to_observed(rows: &[BmSessionRow]) -> Vec<ObservedSession> {
    rows.iter()
        .filter_map(|r| {
            Some(ObservedSession {
                player: r.bm_id.clone(),
                server_id: r.server_id.clone(),
                start: parse_ts(&r.start_upper)?,
                start_known: r.start_known,
                end: if r.is_open { None } else { parse_ts(&r.end_lower) },
                end_known: r.end_known,
            })
        })
        .collect()
}

pub async fn teammates(ctx: &Ctx, bm_id: &str, days: i64) -> AppResult<Vec<TeammateView>> {
    let t = now();
    let (from, to) = (fmt_ts(t - Duration::days(days)), fmt_ts(t));
    let mine = bm_repo::list_sessions_player(&ctx.pool, bm_id, &from, &to, 5000).await?;
    let mut servers: Vec<&str> = mine.iter().map(|s| s.server_id.as_str()).collect();
    servers.sort();
    servers.dedup();
    let mut others = vec![];
    for s in servers {
        others.extend(bm_repo::list_sessions_server(&ctx.pool, s, &from, &to).await?);
    }
    let found = find_teammates(&to_observed(&mine), &to_observed(&others), t, TeammateOptions::default());
    let mut out = Vec::with_capacity(found.len());
    for c in found {
        let p = bm_repo::get_player(&ctx.pool, &c.player).await?;
        out.push(TeammateView {
            name: p.as_ref().and_then(|p| p.name.clone()),
            steam_id: p.as_ref().and_then(|p| p.steam_id.clone()),
            tracked: p.is_some_and(|p| p.tracked),
            candidate: c,
        });
    }
    Ok(out)
}

pub async fn player_view(ctx: &Ctx, input: &str, refresh: bool) -> AppResult<BmPlayerView> {
    let bm = parse_bm_id(input)?;
    let mut notices = vec![];
    let known = bm_repo::get_player(&ctx.pool, &bm.0).await?;
    let stale = known.as_ref().and_then(|p| p.fetched_at.as_deref()).and_then(parse_ts).is_none_or(|f| now() - f > Duration::minutes(10));
    if !ctx.is_demo() && ctx.bm.is_configured() && (refresh || stale) {
        if let Err(e) = refresh_player(ctx, &bm, &mut notices).await {
            if known.is_none() {
                return Err(e);
            }
            notices.push(Notice {
                level: "error", text: format!("Не удалось обновить из BattleMetrics: {e}. Показаны сохранённые данные.")
            });
        }
    } else if known.is_none() {
        return Err(if ctx.is_demo() {
            AppError::not_found("В демо-режиме доступны только вымышленные игроки BM")
        } else {
            AppError::validation("Добавьте токен BattleMetrics.com в «Интеграциях»")
        });
    }
    let player = bm_repo::get_player(&ctx.pool, &bm.0).await?.ok_or_else(|| AppError::not_found("Игрок BattleMetrics не найден"))?;
    let t = now();
    let sessions = bm_repo::list_sessions_player(&ctx.pool, &bm.0, "0000-01-01T00:00:00Z", &fmt_ts(t), 500).await?;
    let (total, observed): (i64, Option<i64>) =
        sqlx::query_as("SELECT COUNT(*), SUM(MAX(0, strftime('%s', end_lower) - strftime('%s', start_upper))) FROM bm_sessions WHERE bm_id = ?")
            .bind(&bm.0)
            .fetch_one(&ctx.pool)
            .await?;
    let servers = bm_repo::list_player_servers(&ctx.pool, &bm.0).await?;
    let mut online_now = servers.iter().find(|s| s.online).cloned();
    if online_now.is_none()
        && let Some(open) = sessions.iter().find(|s| s.is_open)
    {
        online_now = Some(BmServerRow {
            server_id: open.server_id.clone(),
            server_name: open.server_name.clone(),
            online: true,
            first_seen: None,
            last_seen: Some(open.end_lower.clone()),
            time_played_secs: None,
            fetched_at: open.end_lower.clone(),
        });
    }
    let steam = match &player.steam_id {
        Some(s) => repo::get_player(&ctx.pool, s).await?,
        None => None,
    };
    let teammates = teammates(ctx, &bm.0, 90).await?;
    let watched = bm_repo::list_watch(&ctx.pool).await?.iter().filter(|w| w.enabled).count();
    Ok(BmPlayerView {
        source_url: format!("https://www.battlemetrics.com/players/{}", bm.0),
        names: bm_repo::list_names(&ctx.pool, &bm.0).await?,
        sessions_total: total,
        observed_secs: observed.unwrap_or(0),
        teammates_note: format!(
            "Гипотезы по совместным входам/выходам на одном сервере за 90 дней. Учитываются сессии из истории BM и с наблюдаемых серверов ({watched}). Чем дольше идёт наблюдение, тем точнее."
        ),
        watched_servers: watched,
        player,
        servers,
        sessions,
        online_now,
        steam,
        teammates,
        notices,
    })
}

pub async fn activity(ctx: &Ctx, bm_id: &str, range: &RangeSpec, tz: Option<&str>) -> AppResult<ActivityAggregate> {
    let r = range.resolve(now())?;
    let tz: chrono_tz::Tz = match tz {
        Some(t) => t.parse().map_err(|_| AppError::validation("Неизвестный часовой пояс"))?,
        None => ctx.tz(),
    };
    let (f, t) = (fmt_ts(r.start), fmt_ts(r.end));
    let rows = bm_repo::list_sessions_player(&ctx.pool, bm_id, &f, &t, 20000).await?;
    let bounds: Vec<SessionBounds> = rows
        .iter()
        .filter_map(|s| {
            Some(SessionBounds {
                start_lower: parse_ts(&s.start_lower)?,
                start_upper: parse_ts(&s.start_upper)?,
                end_lower: parse_ts(&s.end_lower)?,
                end_upper: s.end_upper.as_deref().and_then(parse_ts),
                server_id: Some(s.server_id.clone()),
            })
        })
        .collect();
    // Покрытие: окна наблюдения серверов + период истории BM (BM видит все серверы, которые отслеживает).
    let mut cov: Vec<Interval> =
        bm_repo::list_coverage_all(&ctx.pool, &f, &t).await?.iter().filter_map(|(a, b)| Interval::new(parse_ts(a)?, parse_ts(b)?)).collect();
    let api: Vec<&BmSessionRow> = rows.iter().filter(|s| s.source == "bm_api").collect();
    if let Some(oldest) = api.iter().filter_map(|s| parse_ts(&s.start_lower)).min()
        && let Some(i) = Interval::new(oldest, r.end)
    {
        cov.push(i);
    }
    let tl = PlayerTimeline::build(&bounds, &cov, r);
    Ok(aggregate_activity(&tl, r, &HeatmapOptions::new(tz)))
}

// ------------------------------------------------------------------ servers

#[derive(Debug, Serialize)]
pub struct OnlinePlayer {
    pub bm_id: String,
    pub name: String,
    pub since: Option<String>,
    pub since_known: bool,
    pub tracked: bool,
    pub steam_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerOnline {
    pub server_id: String,
    pub fetched_at: String,
    pub source: String,
    pub watched: bool,
    pub players: Vec<OnlinePlayer>,
    pub note: Option<String>,
}

pub async fn server_online(ctx: &Ctx, server_id: &str) -> AppResult<ServerOnline> {
    let watched = bm_repo::list_watch(&ctx.pool).await?.iter().any(|w| w.server_id == server_id && w.enabled);
    let open = bm_repo::open_sessions_server(&ctx.pool, server_id).await?;
    let open_by: HashMap<String, &BmSessionRow> = open.iter().map(|s| (s.bm_id.clone(), s)).collect();
    let (list, source, note): (Vec<(String, String)>, &str, Option<String>) =
        if !ctx.is_demo() && ctx.bm.is_configured() && server_id.starts_with("battlemetrics:") {
            let ext = server_id.trim_start_matches("battlemetrics:");
            let r = ctx.bm.server_roster(ext).await.map_err(|e| AppError::provider("battlemetrics", e))?;
            let existing = repo::get_server(&ctx.pool, server_id).await?;
            repo::upsert_server(&ctx.pool, &server_row_from(&r.server, existing.as_ref())).await?;
            for (id, name) in &r.players {
                bm_repo::upsert_player(&ctx.pool, id, Some(name), Some(now()), false).await?;
            }
            (r.players, "battlemetrics", (!watched).then(|| "Время входа известно только для наблюдаемых серверов — включите наблюдение.".into()))
        } else if ctx.is_demo() {
            (open.iter().map(|s| (s.bm_id.clone(), s.player_name.clone().unwrap_or_default())).collect(), "demo", None)
        } else {
            return Err(AppError::validation("Список игроков сервера доступен только для серверов BattleMetrics.com с токеном"));
        };
    let mut players = vec![];
    for (id, name) in list {
        let p = bm_repo::get_player(&ctx.pool, &id).await?;
        let o = open_by.get(&id);
        players.push(OnlinePlayer {
            since: o.map(|s| s.start_upper.clone()),
            since_known: o.is_some_and(|s| s.start_known),
            tracked: p.as_ref().is_some_and(|p| p.tracked),
            steam_id: p.and_then(|p| p.steam_id),
            bm_id: id,
            name,
        });
    }
    players.sort_by(|a, b| b.tracked.cmp(&a.tracked).then(a.since.cmp(&b.since)));
    Ok(ServerOnline { server_id: server_id.into(), fetched_at: fmt_ts(now()), source: source.into(), watched, players, note })
}

pub async fn set_watch(ctx: &Ctx, server_id: &str, enabled: bool) -> AppResult<()> {
    if enabled && !ctx.is_demo() && !server_id.starts_with("battlemetrics:") {
        return Err(AppError::validation("Наблюдать можно только серверы BattleMetrics.com (найдите сервер через каталог с токеном BM)"));
    }
    let n = bm_repo::list_watch(&ctx.pool).await?.iter().filter(|w| w.enabled && w.server_id != server_id).count();
    if enabled && n >= 25 {
        return Err(AppError::validation("Не больше 25 наблюдаемых серверов (лимиты BattleMetrics API)"));
    }
    Ok(bm_repo::set_watch(&ctx.pool, server_id, enabled).await?)
}

pub async fn list_watch(ctx: &Ctx) -> AppResult<Vec<WatchRow>> {
    Ok(bm_repo::list_watch(&ctx.pool).await?)
}

/// Связать игрока BM с профилем Steam (SteamID64, ссылка или vanity).
pub async fn link_steam(ctx: &Ctx, bm_id: &str, steam_input: &str) -> AppResult<Vec<Notice>> {
    let out = player_service::search(ctx, steam_input).await?;
    let sid = out.resolved_steam_id.ok_or_else(|| AppError::validation("Нужен однозначный SteamID64 или ссылка на профиль Steam"))?;
    let mut notices = player_service::link_bm(ctx, &sid, bm_id).await?;
    bm_repo::set_steam(&ctx.pool, bm_id, Some(&sid)).await?;
    notices.push(Notice { level: "info", text: format!("Связано с профилем Steam {sid}") });
    Ok(notices)
}

pub async fn set_tracked(ctx: &Ctx, bm_id: &str, tracked: bool) -> AppResult<()> {
    parse_bm_id(bm_id)?;
    Ok(bm_repo::set_tracked(&ctx.pool, bm_id, tracked).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::demo::demo_bm_id;
    use crate::services::player_service::tests::demo_ctx;

    #[tokio::test]
    async fn demo_teammate_is_found_and_passerby_ranks_lower() {
        let ctx = demo_ctx().await;
        let v = player_view(&ctx, &demo_bm_id(1), false).await.unwrap();
        assert!(v.sessions_total > 20);
        assert!(v.steam.is_some(), "демо-игрок BM связан со Steam");
        let first = &v.teammates[0];
        assert_eq!(first.name.as_deref(), Some("DEMO·Сосед"), "тиммейты: {:?}", v.teammates.iter().map(|t| (&t.name, t.candidate.score)).collect::<Vec<_>>());
        assert!(first.candidate.joined_together >= 5);
        assert_ne!(first.candidate.confidence, "low");
        if let Some(p) = v.teammates.iter().find(|t| t.name.as_deref() == Some("DEMO·Попутчик")) {
            assert!(p.candidate.joined_together <= 2 && p.candidate.score < first.candidate.score);
            assert_ne!(p.candidate.confidence, "high", "попутчик без совместных входов не может быть «высокой»");
        }
        for t in v.teammates.iter().filter(|t| t.name.as_deref().is_some_and(|n| n.starts_with("DEMO·Игрок_"))) {
            assert_ne!(t.candidate.confidence, "high", "случайная массовка: {:?} rate={}", t.name, t.candidate.sync_rate);
        }
    }

    #[tokio::test]
    async fn demo_server_online_and_activity() {
        let ctx = demo_ctx().await;
        let o = server_online(&ctx, "demo:1").await.unwrap();
        assert!(o.watched);
        let a = activity(&ctx, &demo_bm_id(1), &RangeSpec { preset: "30d".into(), from: None, to: None }, Some("Europe/Moscow")).await.unwrap();
        assert!(a.coverage > 0.3 && a.online_secs > 0);
        assert!(player_view(&ctx, "9999999", false).await.is_err());
    }
}
