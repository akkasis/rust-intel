//! Отчёты активности и анализ отрядов.

use super::{RangeSpec, to_bounds, to_intervals};
use crate::domain::analytics::{
    ActivityAggregate, HeatmapOptions, Interval, PlayerTimeline, SquadAnalysis, aggregate_activity, analyze_squad, co_sessions, total_secs,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::{fmt_ts, now};
use crate::persistence::models::{SessionRow, SquadMemberRow, SquadRow};
use crate::persistence::repo;
use crate::state::Ctx;
use chrono_tz::Tz;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ActivityReport {
    pub steam_id: String,
    pub aggregate: ActivityAggregate,
    pub sessions: Vec<SessionRow>,
    pub sessions_truncated: bool,
    pub sources: Vec<String>,
    pub servers: Vec<(String, Option<String>)>,
    pub server_filter: Option<String>,
    pub poll_interval_secs: Option<i64>,
    pub is_demo: bool,
}

fn tz_or(ctx: &Ctx, tz: Option<&str>) -> AppResult<Tz> {
    match tz {
        Some(t) => t.parse().map_err(|_| AppError::validation(format!("Неизвестный часовой пояс: {t}"))),
        None => Ok(ctx.tz()),
    }
}

async fn timeline(ctx: &Ctx, steam_id: &str, range: Interval, server: Option<&str>) -> AppResult<(PlayerTimeline, Vec<SessionRow>, Vec<String>)> {
    let (f, t) = (fmt_ts(range.start), fmt_ts(range.end));
    let mut sessions = repo::list_sessions(&ctx.pool, steam_id, &f, &t).await?;
    let cov = repo::list_coverage(&ctx.pool, steam_id, &f, &t).await?;
    let mut sources: Vec<String> = cov.iter().map(|c| c.provider.clone()).chain(sessions.iter().map(|s| s.provider.clone())).collect();
    sources.sort();
    sources.dedup();
    if let Some(sv) = server {
        sessions.retain(|s| s.server_id.as_deref() == Some(sv));
    }
    let tl = PlayerTimeline::build(&to_bounds(&sessions), &to_intervals(&cov), range);
    Ok((tl, sessions, sources))
}

pub async fn activity(ctx: &Ctx, steam_id: &str, range: &RangeSpec, tz: Option<&str>, server: Option<&str>) -> AppResult<ActivityReport> {
    let p = super::player_service::ensure_known_player(ctx, steam_id).await?;
    let r = range.resolve(now())?;
    let tz = tz_or(ctx, tz)?;
    let (tl, mut sessions, sources) = timeline(ctx, steam_id, r, server).await?;
    let mut opt = HeatmapOptions::new(tz);
    // для коротких периодов требуем меньше покрытия на ячейку
    if r.secs() <= 2 * 86_400 {
        opt.min_known_secs = 10 * 60;
    }
    let aggregate = aggregate_activity(&tl, r, &opt);
    let all = repo::list_sessions(&ctx.pool, steam_id, &fmt_ts(now() - chrono::Duration::days(400)), &fmt_ts(now())).await?;
    let mut servers: Vec<(String, Option<String>)> = all.iter().filter_map(|s| s.server_id.clone().map(|id| (id, s.server_name.clone()))).collect();
    servers.sort();
    servers.dedup_by(|a, b| a.0 == b.0);
    let poll: Option<i64> =
        sqlx::query_scalar("SELECT MAX(poll_interval_secs) FROM observations WHERE steam_id = ?").bind(steam_id).fetch_one(&ctx.pool).await.ok().flatten();
    let truncated = sessions.len() > 500;
    sessions.truncate(500);
    Ok(ActivityReport {
        steam_id: steam_id.into(),
        aggregate,
        sessions,
        sessions_truncated: truncated,
        sources,
        servers,
        server_filter: server.map(str::to_string),
        poll_interval_secs: poll.or(if p.is_demo { Some(300) } else { None }),
        is_demo: p.is_demo,
    })
}

#[derive(Debug, Serialize)]
pub struct MemberCoverage {
    pub steam_id: String,
    pub known_secs: i64,
    pub online_secs: i64,
    pub coverage: f64,
}

#[derive(Debug, Serialize)]
pub struct TeammateSuggestion {
    pub steam_id: String,
    pub persona_name: Option<String>,
    pub co_sessions: usize,
    pub with_members: Vec<String>,
    pub co_online_secs: i64,
    pub evidence: Vec<crate::domain::analytics::CoSessionEvidence>,
}

#[derive(Debug, Serialize)]
pub struct SquadReport {
    pub squad: SquadRow,
    pub members: Vec<SquadMemberRow>,
    pub coverage: Vec<MemberCoverage>,
    pub analysis: Option<SquadAnalysis>,
    pub suggestions: Vec<TeammateSuggestion>,
    pub timezone: String,
}

pub async fn squad_report(ctx: &Ctx, squad_id: i64, range: &RangeSpec, tz: Option<&str>) -> AppResult<SquadReport> {
    let squad = repo::get_squad(&ctx.pool, squad_id).await?.ok_or_else(|| AppError::not_found("Отряд не найден"))?;
    let members = repo::list_squad_members(&ctx.pool, squad_id).await?;
    let r = range.resolve(now())?;
    let tz = tz_or(ctx, tz)?;
    let mut tls = Vec::new();
    let mut coverage = Vec::new();
    let mut member_sessions = Vec::new();
    for m in &members {
        let (tl, sessions, _) = timeline(ctx, &m.steam_id, r, None).await?;
        coverage.push(MemberCoverage {
            steam_id: m.steam_id.clone(),
            known_secs: total_secs(&tl.known),
            online_secs: total_secs(&tl.online),
            coverage: total_secs(&tl.known) as f64 / r.secs().max(1) as f64,
        });
        member_sessions.push(to_bounds(&sessions));
        tls.push(tl);
    }
    let analysis = (!members.is_empty()).then(|| analyze_squad(&tls, r, &HeatmapOptions::new(tz)));

    // Предложения: игроки из базы с ≥ 3 совместными сессиями на одном сервере с участниками.
    let mut suggestions = Vec::new();
    let member_ids: std::collections::HashSet<&str> = members.iter().map(|m| m.steam_id.as_str()).collect();
    for other in repo::list_players(&ctx.pool, None, false, 500, 0).await? {
        if member_ids.contains(other.steam_id.as_str()) {
            continue;
        }
        let theirs = to_bounds(&repo::list_sessions(&ctx.pool, &other.steam_id, &fmt_ts(r.start), &fmt_ts(r.end)).await?);
        if theirs.is_empty() {
            continue;
        }
        let mut evidence = Vec::new();
        let mut with = Vec::new();
        for (m, ms) in members.iter().zip(&member_sessions) {
            let ev = co_sessions(ms, &theirs, 20 * 60);
            if !ev.is_empty() {
                with.push(m.persona_name.clone().unwrap_or_else(|| m.steam_id.clone()));
            }
            evidence.extend(ev);
        }
        if evidence.len() >= 3 {
            evidence.sort_by_key(|e| std::cmp::Reverse(e.start));
            suggestions.push(TeammateSuggestion {
                steam_id: other.steam_id,
                persona_name: other.persona_name,
                co_sessions: evidence.len(),
                with_members: with,
                co_online_secs: evidence.iter().map(|e| e.overlap_secs).sum(),
                evidence: evidence.into_iter().take(5).collect(),
            });
        }
    }
    suggestions.sort_by_key(|x| std::cmp::Reverse(x.co_sessions));
    Ok(SquadReport { squad, members, coverage, analysis, suggestions, timezone: tz.name().into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::demo::demo_steam_id;
    use crate::services::player_service::tests::demo_ctx;

    #[tokio::test]
    async fn demo_activity_has_unknowns_and_coverage() {
        let ctx = demo_ctx().await;
        let r =
            activity(&ctx, demo_steam_id(1).as_str(), &RangeSpec { preset: "90d".into(), from: None, to: None }, Some("Europe/Moscow"), None).await.unwrap();
        assert!(r.aggregate.coverage > 0.5 && r.aggregate.coverage < 1.0, "покрытие с пробелами: {}", r.aggregate.coverage);
        assert!(r.aggregate.online_secs > 0);
        let r6 = activity(&ctx, demo_steam_id(6).as_str(), &RangeSpec { preset: "90d".into(), from: None, to: None }, None, None).await.unwrap();
        assert!(matches!(r6.aggregate.typical_offline, crate::domain::analytics::TypicalOffline::InsufficientData { .. }));
    }

    #[tokio::test]
    async fn demo_squad_report() {
        let ctx = demo_ctx().await;
        let squads = repo::list_squads(&ctx.pool).await.unwrap();
        let r = squad_report(&ctx, squads[0].id, &RangeSpec { preset: "30d".into(), from: None, to: None }, None).await.unwrap();
        let a = r.analysis.unwrap();
        assert!(a.some_online_secs > 0);
        assert!(a.unknown_secs > 0, "пробелы покрытия → неизвестно");
    }

    #[tokio::test]
    async fn rejects_bad_range_and_tz() {
        let ctx = demo_ctx().await;
        let id = demo_steam_id(1);
        assert!(activity(&ctx, id.as_str(), &RangeSpec { preset: "1y".into(), from: None, to: None }, None, None).await.is_err());
        assert!(activity(&ctx, id.as_str(), &RangeSpec { preset: "7d".into(), from: None, to: None }, Some("Bad/Zone"), None).await.is_err());
    }
}
