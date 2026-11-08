//! Каталог серверов (серверный уровень). История онлайна сервера ≠ история игроков.

use crate::domain::provenance::{DataStatus, Provenance};
use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::{fmt_ts, now, parse_ts};
use crate::persistence::models::{PopulationPoint, ServerRow};
use crate::persistence::repo;
use crate::providers::{ServerCatalog, ServerInfo};
use crate::state::Ctx;
use serde::Serialize;
use std::sync::Arc;

fn to_row(s: ServerInfo, followed: bool) -> ServerRow {
    ServerRow {
        id: format!("{}:{}", s.provider, s.external_id),
        provider: s.provider.into(),
        external_id: s.external_id,
        name: s.name,
        address: s.address,
        country: s.country,
        map: s.map,
        players: s.players,
        max_players: s.max_players,
        queue: s.queue,
        online: s.online,
        trust: s.trust,
        last_wipe_at: s.last_wipe_at.map(fmt_ts),
        next_wipe_at: s.next_wipe_at.map(fmt_ts),
        followed,
        source_url: s.source_url,
        fetched_at: fmt_ts(now()),
        is_demo: false,
    }
}

fn catalogs(ctx: &Ctx) -> Vec<Arc<dyn ServerCatalog>> {
    let mut v: Vec<Arc<dyn ServerCatalog>> = Vec::new();
    if ctx.bmru.is_configured() {
        v.push(ctx.bmru.clone());
    }
    if ctx.bm.is_configured() {
        v.push(ctx.bm.clone());
    }
    v
}

#[derive(Debug, Serialize)]
pub struct RefreshResult {
    pub fetched: usize,
    pub errors: Vec<String>,
    pub sources: Vec<String>,
}

pub async fn refresh(ctx: &Ctx, query: Option<&str>) -> AppResult<RefreshResult> {
    if ctx.is_demo() {
        return Ok(RefreshResult {
            fetched: 0, errors: vec!["Демо-режим: каталог вымышленный и не обновляется из сети".into()], sources: vec!["demo".into()]
        });
    }
    let cats = catalogs(ctx);
    if cats.is_empty() {
        return Err(AppError::validation("Нет настроенного каталога серверов: добавьте ключ battlemetrics.ru (бесплатно) или токен BattleMetrics.com"));
    }
    let mut res = RefreshResult { fetched: 0, errors: vec![], sources: vec![] };
    for c in cats {
        res.sources.push(c.id().into());
        match c.search_servers(query).await {
            Ok(list) => {
                repo::record_health_ok(&ctx.pool, c.id()).await?;
                for s in list {
                    let id = format!("{}:{}", s.provider, s.external_id);
                    let followed = repo::get_server(&ctx.pool, &id).await?.is_some_and(|x| x.followed);
                    repo::upsert_server(&ctx.pool, &to_row(s, followed)).await?;
                    res.fetched += 1;
                }
            }
            Err(e) => {
                crate::services::player_service::record_err(ctx, c.id(), &e).await;
                res.errors.push(format!("{}: {e}", c.id()));
            }
        }
    }
    // Вайпы для отслеживаемых серверов battlemetrics.ru
    if ctx.bmru.is_configured()
        && let Ok(w) = ctx.bmru.wipes("upcoming").await
    {
        for (sid, at, _) in w {
            sqlx::query("UPDATE servers SET next_wipe_at = ? WHERE id = ?").bind(fmt_ts(at)).bind(format!("battlemetrics_ru:{sid}")).execute(&ctx.pool).await?;
        }
    }
    Ok(res)
}

#[derive(Debug, Serialize)]
pub struct PopulationReport {
    pub server: ServerRow,
    pub period: String,
    pub points: Vec<PopulationPoint>,
    pub provenance: Provenance,
}

pub async fn population(ctx: &Ctx, server_id: &str, period: &str) -> AppResult<PopulationReport> {
    let server = repo::get_server(&ctx.pool, server_id).await?.ok_or_else(|| AppError::not_found("Сервер не найден"))?;
    let period = match period {
        "24h" | "7d" | "30d" => period,
        _ => "7d",
    };
    let since = fmt_ts(now() - super::period_duration(period));
    let mut prov = Provenance::new(&server.provider, if server.is_demo { DataStatus::Demo } else { DataStatus::Stale });
    if !ctx.is_demo() {
        let cat: Option<Arc<dyn ServerCatalog>> = match server.provider.as_str() {
            "battlemetrics_ru" if ctx.bmru.is_configured() => Some(ctx.bmru.clone()),
            "battlemetrics" if ctx.bm.is_configured() => Some(ctx.bm.clone()),
            _ => None,
        };
        match cat {
            Some(c) => match c.population(&server.external_id, period).await {
                Ok(pts) => {
                    let rows: Vec<(String, f64)> = pts.into_iter().map(|(t, p)| (fmt_ts(t), p)).collect();
                    repo::insert_population(&ctx.pool, server_id, &rows, c.id()).await?;
                    prov = Provenance::new(c.id(), DataStatus::Verified).fetched(now());
                }
                Err(e) => {
                    prov = prov.note(format!("Обновление не удалось: {e}. Показан кэш."));
                }
            },
            None => prov = Provenance::new(&server.provider, DataStatus::NotConfigured).note("Источник сервера не настроен — показан кэш"),
        }
    }
    prov.source_url = server.source_url.clone();
    if prov.fetched_at.is_none() {
        prov.fetched_at = parse_ts(&server.fetched_at);
    }
    let points = repo::get_population(&ctx.pool, server_id, &since).await?;
    Ok(PopulationReport { server, period: period.into(), points, provenance: prov })
}
