//! Распорядок, кланы BM, детектор нового аккаунта, активность после вайпа, топ/новички сервера.

use super::RangeSpec;
use crate::domain::analytics::{HeatmapOptions, Interval, PlayerTimeline, SessionBounds, SquadAnalysis, analyze_squad};
use crate::domain::insights::{PostWipe, Routine, cosine, post_wipe, routine, week_profile};
use crate::domain::teammates::{ObservedSession, TeammateOptions, find_teammates};
use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::{fmt_ts, now, parse_ts};
use crate::persistence::bm_repo::{self, BmSessionRow};
use crate::persistence::repo::{self, NewEvent};
use crate::services::bm_service;
use crate::state::Ctx;
use chrono::{Duration, DurationRound};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::{HashMap, HashSet};

fn tz_of(ctx: &Ctx, tz: Option<&str>) -> AppResult<Tz> {
    match tz {
        Some(t) => t.parse().map_err(|_| AppError::validation("Неизвестный часовой пояс")),
        None => Ok(ctx.tz()),
    }
}

fn observed(rows: &[BmSessionRow]) -> Vec<ObservedSession> {
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

async fn sessions_since(ctx: &Ctx, bm_id: &str, days: i64) -> AppResult<Vec<BmSessionRow>> {
    let t = now();
    Ok(bm_repo::list_sessions_player(&ctx.pool, bm_id, &fmt_ts(t - Duration::days(days)), &fmt_ts(t), 20000).await?)
}

// ------------------------------------------------------------------ 1. распорядок

pub async fn player_routine(ctx: &Ctx, bm_id: &str, tz: Option<&str>) -> AppResult<Routine> {
    let tz = tz_of(ctx, tz)?;
    let agg = bm_service::activity(ctx, bm_id, &RangeSpec { preset: "90d".into(), from: None, to: None }, Some(tz.name())).await?;
    let rows = sessions_since(ctx, bm_id, 60).await?;
    Ok(routine(&observed(&rows), &agg, tz, now()))
}

// ------------------------------------------------------------------ 3. после вайпа

pub async fn player_post_wipe(ctx: &Ctx, bm_id: &str, tz: Option<&str>) -> AppResult<PostWipe> {
    let tz = tz_of(ctx, tz)?;
    let rows = sessions_since(ctx, bm_id, 120).await?;
    let mut servers: Vec<String> = rows.iter().map(|r| r.server_id.clone()).collect();
    servers.sort();
    servers.dedup();
    let mut wipes = vec![];
    let from = fmt_ts(now() - Duration::days(120));
    for s in &servers {
        let list: Vec<(String,)> =
            sqlx::query_as("SELECT at FROM server_wipes WHERE server_id = ? AND at >= ? ORDER BY at").bind(s).bind(&from).fetch_all(&ctx.pool).await?;
        wipes.extend(list.into_iter().filter_map(|(a,)| parse_ts(&a).map(|t| (s.clone(), t))));
    }
    Ok(post_wipe(&observed(&rows), &wipes, tz, now()))
}

// ------------------------------------------------------------------ 4. новый аккаунт

#[derive(Debug, Serialize)]
pub struct AltCandidate {
    pub bm_id: String,
    pub name: Option<String>,
    pub first_seen: Option<String>,
    /// Совместные входы/выходы с целью и её тиммейтами.
    pub sync_with_group: u32,
    pub with_whom: Vec<String>,
    /// Похожесть недельного расписания с целью (0..1).
    pub schedule_similarity: f64,
    pub target_inactive_since: Option<String>,
    pub score: f64,
    pub reasons: Vec<String>,
}

pub async fn alt_candidates(ctx: &Ctx, bm_id: &str) -> AppResult<Vec<AltCandidate>> {
    let t = now();
    let tz = ctx.tz();
    let target_rows = sessions_since(ctx, bm_id, 90).await?;
    if target_rows.is_empty() {
        return Ok(vec![]);
    }
    let target = observed(&target_rows);
    let target_profile = week_profile(&target, tz, t);
    let target_last = target.iter().map(|s| s.end.unwrap_or(t)).max();
    let target_inactive = target_last.filter(|l| t - *l > Duration::days(3));
    let mates: Vec<String> =
        bm_service::teammates(ctx, bm_id, 90).await?.into_iter().filter(|m| m.candidate.confidence != "low").map(|m| m.candidate.player).collect();
    let mut group_rows = target_rows.clone();
    for m in &mates {
        group_rows.extend(sessions_since(ctx, m, 90).await?);
    }
    let group = observed(&group_rows);
    let mut names: HashMap<String, String> = HashMap::new();
    for r in &group_rows {
        if let Some(n) = &r.player_name {
            names.insert(r.bm_id.clone(), n.clone());
        }
    }
    let servers: HashSet<&str> = target_rows.iter().map(|r| r.server_id.as_str()).collect();
    let since = fmt_ts(t - Duration::days(30));
    let mut new_players: Vec<(String, String)> = vec![];
    for s in &servers {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT bm_id, MIN(start_upper) FROM bm_sessions WHERE server_id = ? GROUP BY bm_id
             HAVING MIN(start_upper) >= ? AND bm_id NOT IN (SELECT bm_id FROM bm_sessions WHERE start_upper < ?)",
        )
        .bind(s)
        .bind(&since)
        .bind(&since)
        .fetch_all(&ctx.pool)
        .await?;
        new_players.extend(rows);
    }
    let exclude: HashSet<&str> = mates.iter().map(String::as_str).chain([bm_id]).collect();
    new_players.retain(|(id, _)| !exclude.contains(id.as_str()));
    new_players.sort();
    new_players.dedup_by(|a, b| a.0 == b.0);
    new_players.truncate(400);
    let mut out = vec![];
    for (id, first) in new_players {
        let rows = sessions_since(ctx, &id, 30).await?;
        let xs = observed(&rows);
        if xs.len() < 2 {
            continue;
        }
        let found = find_teammates(&xs, &group, t, TeammateOptions { min_overlap_secs: 20 * 60, window_secs: 600, max_results: 50 });
        let sync: u32 = found.iter().map(|c| c.joined_together + c.left_together).sum();
        let with: Vec<String> =
            found.iter().filter(|c| c.joined_together + c.left_together > 0).map(|c| names.get(&c.player).cloned().unwrap_or(c.player.clone())).collect();
        let sim = cosine(&week_profile(&xs, tz, t), &target_profile);
        let first_ts = parse_ts(&first);
        let replaced = target_inactive.zip(first_ts).is_some_and(|(ti, f)| f >= ti - Duration::days(2));
        let mut reasons = vec![];
        if sync >= 2 {
            reasons.push(format!("заходит/выходит вместе с группой цели: {sync} раз"));
        }
        if sim >= 0.6 {
            reasons.push(format!("расписание похоже на цель на {:.0}%", sim * 100.0));
        }
        if replaced {
            reasons.push("появился, когда цель перестала заходить".into());
        }
        if sync < 2 && !(sim >= 0.7 && replaced) {
            continue;
        }
        let score = sync as f64 * 2.0 + sim * 5.0 + if replaced { 4.0 } else { 0.0 };
        let p = bm_repo::get_player(&ctx.pool, &id).await?;
        out.push(AltCandidate {
            name: p.and_then(|p| p.name),
            bm_id: id,
            first_seen: Some(first),
            sync_with_group: sync,
            with_whom: with,
            schedule_similarity: sim,
            target_inactive_since: target_inactive.map(fmt_ts),
            score,
            reasons,
        });
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(15);
    Ok(out)
}

// ------------------------------------------------------------------ 2. кланы

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ClanRow {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub notify: bool,
    pub min_gather: i64,
    pub created_at: String,
    pub updated_at: String,
    pub member_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ClanMemberRow {
    pub bm_id: String,
    pub name: Option<String>,
    pub provenance: String,
    pub added_at: String,
    pub tracked: bool,
    pub is_demo: bool,
    pub online_server: Option<String>,
}

pub async fn list_clans(ctx: &Ctx) -> AppResult<Vec<ClanRow>> {
    Ok(sqlx::query_as("SELECT c.*, (SELECT COUNT(*) FROM bm_clan_members m WHERE m.clan_id = c.id) AS member_count FROM bm_clans c ORDER BY c.name")
        .fetch_all(&ctx.pool)
        .await?)
}

pub async fn clan_members(ctx: &Ctx, id: i64) -> AppResult<Vec<ClanMemberRow>> {
    Ok(sqlx::query_as(
        "SELECT m.bm_id, p.name, m.provenance, m.added_at, p.tracked, p.is_demo,
                (SELECT COALESCE(sv.name, s.server_id) FROM bm_sessions s LEFT JOIN servers sv ON sv.id = s.server_id
                  WHERE s.bm_id = m.bm_id AND s.is_open = 1 LIMIT 1) AS online_server
         FROM bm_clan_members m JOIN bm_players p ON p.bm_id = m.bm_id WHERE m.clan_id = ? ORDER BY m.added_at",
    )
    .bind(id)
    .fetch_all(&ctx.pool)
    .await?)
}

pub async fn save_clan(ctx: &Ctx, id: Option<i64>, name: &str, color: &str, notify: bool, min_gather: i64) -> AppResult<i64> {
    let name = crate::infrastructure::redact::sanitize_text(name, 64);
    if name.is_empty() {
        return Err(AppError::validation("Название клана не может быть пустым"));
    }
    let color = if color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit()) { color } else { "#2dd4bf" };
    let ts = fmt_ts(now());
    let mg = min_gather.clamp(2, 20);
    Ok(match id {
        Some(id) => {
            sqlx::query("UPDATE bm_clans SET name = ?, color = ?, notify = ?, min_gather = ?, updated_at = ? WHERE id = ?")
                .bind(&name)
                .bind(color)
                .bind(notify)
                .bind(mg)
                .bind(&ts)
                .bind(id)
                .execute(&ctx.pool)
                .await?;
            id
        }
        None => sqlx::query("INSERT INTO bm_clans(name, color, notify, min_gather, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&name)
            .bind(color)
            .bind(notify)
            .bind(mg)
            .bind(&ts)
            .bind(&ts)
            .execute(&ctx.pool)
            .await?
            .last_insert_rowid(),
    })
}

pub async fn delete_clan(ctx: &Ctx, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM bm_clans WHERE id = ?").bind(id).execute(&ctx.pool).await?;
    Ok(())
}

pub async fn add_clan_member(ctx: &Ctx, id: i64, bm_id: &str, provenance: &str) -> AppResult<()> {
    bm_service::parse_bm_id(bm_id)?;
    if bm_repo::get_player(&ctx.pool, bm_id).await?.is_none() {
        return Err(AppError::not_found("Игрок BM не найден в базе — откройте его карточку"));
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bm_clan_members WHERE clan_id = ?").bind(id).fetch_one(&ctx.pool).await?;
    if n >= 30 {
        return Err(AppError::validation("В клане не больше 30 участников"));
    }
    sqlx::query("INSERT OR IGNORE INTO bm_clan_members(clan_id, bm_id, provenance, added_at) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(bm_id)
        .bind(provenance)
        .bind(fmt_ts(now()))
        .execute(&ctx.pool)
        .await?;
    Ok(())
}

pub async fn remove_clan_member(ctx: &Ctx, id: i64, bm_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM bm_clan_members WHERE clan_id = ? AND bm_id = ?").bind(id).bind(bm_id).execute(&ctx.pool).await?;
    Ok(())
}

/// Создаёт клан из игрока и его тиммейтов со средней/высокой уверенностью.
pub async fn clan_from_teammates(ctx: &Ctx, bm_id: &str) -> AppResult<i64> {
    let p = bm_repo::get_player(&ctx.pool, bm_id).await?.ok_or_else(|| AppError::not_found("Игрок BM не найден"))?;
    let mates: Vec<_> = bm_service::teammates(ctx, bm_id, 90).await?.into_iter().filter(|m| m.candidate.confidence != "low").collect();
    if mates.is_empty() {
        return Err(AppError::validation("Нет тиммейтов со средней или высокой уверенностью — пока не из кого собирать клан"));
    }
    let id = save_clan(ctx, None, &format!("Клан {}", p.name.as_deref().unwrap_or(bm_id)), "#a3e635", true, 2).await?;
    add_clan_member(ctx, id, bm_id, "manual").await?;
    for m in mates.iter().take(29) {
        add_clan_member(ctx, id, &m.candidate.player, "teammate_suggestion").await?;
    }
    Ok(id)
}

#[derive(Debug, Serialize)]
pub struct ClanReport {
    pub clan: ClanRow,
    pub members: Vec<ClanMemberRow>,
    pub coverage: Vec<f64>,
    pub analysis: Option<SquadAnalysis>,
    pub online_now: usize,
    pub timezone: String,
}

pub async fn clan_report(ctx: &Ctx, id: i64, range: &RangeSpec, tz: Option<&str>) -> AppResult<ClanReport> {
    let clan = list_clans(ctx).await?.into_iter().find(|c| c.id == id).ok_or_else(|| AppError::not_found("Клан не найден"))?;
    let members = clan_members(ctx, id).await?;
    let tz = tz_of(ctx, tz)?;
    let r = range.resolve(now())?;
    let (f, t) = (fmt_ts(r.start), fmt_ts(r.end));
    let server_cov: Vec<Interval> =
        bm_repo::list_coverage_all(&ctx.pool, &f, &t).await?.iter().filter_map(|(a, b)| Interval::new(parse_ts(a)?, parse_ts(b)?)).collect();
    let mut tls = vec![];
    let mut coverage = vec![];
    for m in &members {
        let rows = bm_repo::list_sessions_player(&ctx.pool, &m.bm_id, &f, &t, 20000).await?;
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
        let mut cov = server_cov.clone();
        if let Some(o) = rows.iter().filter(|s| s.source == "bm_api").filter_map(|s| parse_ts(&s.start_lower)).min()
            && let Some(i) = Interval::new(o, r.end)
        {
            cov.push(i);
        }
        let tl = PlayerTimeline::build(&bounds, &cov, r);
        coverage.push(crate::domain::analytics::total_secs(&tl.known) as f64 / r.secs().max(1) as f64);
        tls.push(tl);
    }
    let analysis = (!members.is_empty()).then(|| analyze_squad(&tls, r, &HeatmapOptions::new(tz)));
    Ok(ClanReport { online_now: members.iter().filter(|m| m.online_server.is_some()).count(), clan, members, coverage, analysis, timezone: tz.name().into() })
}

/// «Клан заходит»: ≥ min_gather участников вошли (наблюдаемо) за последние 10 минут.
pub async fn check_clan_gatherings(ctx: &Ctx) -> AppResult<Vec<i64>> {
    let t = now();
    let since = fmt_ts(t - Duration::minutes(10));
    let mut out = vec![];
    for c in list_clans(ctx).await?.into_iter().filter(|c| c.notify) {
        let joined: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT s.bm_id, COALESCE(p.name, s.bm_id), s.server_id FROM bm_sessions s
             JOIN bm_clan_members m ON m.bm_id = s.bm_id AND m.clan_id = ? JOIN bm_players p ON p.bm_id = s.bm_id
             WHERE s.start_known = 1 AND s.start_upper >= ? GROUP BY s.bm_id",
        )
        .bind(c.id)
        .bind(&since)
        .fetch_all(&ctx.pool)
        .await?;
        if (joined.len() as i64) < c.min_gather {
            continue;
        }
        let bucket = t.duration_trunc(Duration::minutes(30)).unwrap_or(t);
        let names: Vec<&str> = joined.iter().map(|j| j.1.as_str()).collect();
        if let Some(e) = repo::insert_event(
            &ctx.pool,
            &NewEvent {
                steam_id: None,
                provider: "battlemetrics",
                kind: "clan_gathering",
                dedup_key: format!("clan:{}:{}", c.id, fmt_ts(bucket)),
                occurred_lower: t - Duration::minutes(10),
                occurred_upper: t,
                approximate: true,
                server_id: joined.first().map(|j| j.2.as_str()),
                details: serde_json::json!({"clan_id": c.id, "name": c.name, "members": names, "count": joined.len()}),
            },
        )
        .await?
        {
            out.push(e);
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ 5. топ и новички сервера

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ServerPlayerStat {
    pub bm_id: String,
    pub name: Option<String>,
    pub secs: i64,
    pub sessions: i64,
    pub first_seen: String,
    pub tracked: bool,
}

#[derive(Debug, Serialize)]
pub struct ServerInsights {
    pub server_id: String,
    pub days: i64,
    pub top: Vec<ServerPlayerStat>,
    pub newcomers: Vec<ServerPlayerStat>,
    pub observed_since: Option<String>,
    pub note: Option<String>,
}

pub async fn server_insights(ctx: &Ctx, server_id: &str, days: i64) -> AppResult<ServerInsights> {
    let days = days.clamp(1, 30);
    let t = now();
    let since = fmt_ts(t - Duration::days(days));
    let q = "SELECT s.bm_id, p.name, SUM(MAX(0, strftime('%s', CASE WHEN s.is_open = 1 THEN ? ELSE s.end_lower END) - strftime('%s', MAX(s.start_upper, ?)))) AS secs,
                    COUNT(*) AS sessions, (SELECT MIN(x.start_upper) FROM bm_sessions x WHERE x.bm_id = s.bm_id AND x.server_id = s.server_id) AS first_seen,
                    p.tracked
             FROM bm_sessions s JOIN bm_players p ON p.bm_id = s.bm_id
             WHERE s.server_id = ? AND s.end_lower >= ? GROUP BY s.bm_id";
    let mut all: Vec<ServerPlayerStat> = sqlx::query_as(q).bind(fmt_ts(t)).bind(&since).bind(server_id).bind(&since).fetch_all(&ctx.pool).await?;
    let observed_since: Option<String> =
        sqlx::query_scalar("SELECT MIN(start_at) FROM server_coverage WHERE server_id = ?").bind(server_id).fetch_one(&ctx.pool).await?;
    let watch_old_enough = observed_since.as_deref().and_then(parse_ts).is_some_and(|o| o <= t - Duration::days(days));
    all.sort_by_key(|s| std::cmp::Reverse(s.secs));
    let mut newcomers: Vec<ServerPlayerStat> = if watch_old_enough { all.iter().filter(|s| s.first_seen >= since).cloned().collect() } else { vec![] };
    newcomers.sort_by(|a, b| b.first_seen.cmp(&a.first_seen));
    newcomers.truncate(50);
    all.truncate(25);
    Ok(ServerInsights {
        server_id: server_id.into(),
        days,
        top: all,
        newcomers,
        note: (!watch_old_enough)
            .then(|| format!("Сервер наблюдается меньше {days} дн. — топ неполный, а «новичков» определить нельзя (все выглядели бы новыми).")),
        observed_since,
    })
}

/// Запоминает вайп сервера (вызывается при любом обновлении сервера).
pub async fn record_wipe(pool: &sqlx::SqlitePool, server_id: &str, at: &str, source: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO server_wipes(server_id, at, source) VALUES (?, ?, ?)").bind(server_id).bind(at).bind(source).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::demo::demo_bm_id;
    use crate::services::player_service::tests::demo_ctx;

    #[tokio::test]
    async fn demo_insights_end_to_end() {
        let ctx = demo_ctx().await;
        let r = player_routine(&ctx, &demo_bm_id(1), Some("Europe/Moscow")).await.unwrap();
        assert!(r.sessions > 10 && r.usual_start.is_some(), "{:?}", r.summary);
        let pw = player_post_wipe(&ctx, &demo_bm_id(1), Some("Europe/Moscow")).await.unwrap();
        assert!(pw.cycles.len() >= 4, "вайпы по четвергам: {}", pw.cycles.len());
        let alts = alt_candidates(&ctx, &demo_bm_id(1)).await.unwrap();
        assert!(alts.iter().any(|a| a.name.as_deref() == Some("DEMO·Новый_акк")), "{:?}", alts.iter().map(|a| &a.name).collect::<Vec<_>>());
        let clans = list_clans(&ctx).await.unwrap();
        assert_eq!(clans.len(), 1);
        let rep = clan_report(&ctx, clans[0].id, &RangeSpec { preset: "30d".into(), from: None, to: None }, None).await.unwrap();
        assert_eq!(rep.members.len(), 4);
        assert!(rep.analysis.unwrap().some_online_secs > 0);
        let si = server_insights(&ctx, "demo:1", 7).await.unwrap();
        assert!(!si.top.is_empty());
        assert!(si.newcomers.iter().all(|n| n.first_seen >= fmt_ts(now() - Duration::days(7))));
        let id = clan_from_teammates(&ctx, &demo_bm_id(1)).await.unwrap();
        assert!(clan_members(&ctx, id).await.unwrap().len() >= 3);
    }

    #[tokio::test]
    async fn clan_gathering_event_is_deduplicated() {
        let ctx = demo_ctx().await;
        let clans = list_clans(&ctx).await.unwrap();
        let t = now();
        for bm in [demo_bm_id(1), demo_bm_id(2)] {
            bm_repo::upsert_session(
                &ctx.pool,
                &bm_repo::NewBmSession {
                    bm_id: &bm,
                    server_id: "demo:1",
                    source: "server_watch",
                    start_lower: t - Duration::minutes(4),
                    start_upper: t - Duration::minutes(2),
                    start_known: true,
                    end_lower: t,
                    end_upper: None,
                    end_known: false,
                    is_open: true,
                    external_id: None,
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(clans.len(), 1);
        let a = check_clan_gatherings(&ctx).await.unwrap();
        let b = check_clan_gatherings(&ctx).await.unwrap();
        assert_eq!(a.len(), 1);
        assert!(b.is_empty(), "без дублей");
    }
}
