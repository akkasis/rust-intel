//! Экспорт/импорт. Секреты не экспортируются никогда. CSV защищён от formula injection.

use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::{fmt_ts, now};
use crate::persistence::models::*;
use crate::persistence::repo;
use crate::state::Ctx;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EXPORT_FORMAT: &str = "rid-export";
pub const EXPORT_VERSION: i64 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportBundle {
    pub format: String,
    pub version: i64,
    pub exported_at: String,
    pub mode: String,
    pub players: Vec<PlayerRow>,
    pub aliases: Vec<(String, AliasRow)>,
    pub bans: Vec<BanRow>,
    pub sessions: Vec<SessionRow>,
    pub coverage: Vec<(String, CoverageRow)>,
    pub squads: Vec<(SquadRow, Vec<SquadMemberRow>)>,
    pub servers: Vec<ServerRow>,
    pub events: Vec<EventRow>,
}

pub async fn build_bundle(ctx: &Ctx) -> AppResult<ExportBundle> {
    let players = repo::list_players(&ctx.pool, None, false, 100_000, 0).await?;
    let (lo, hi) = ("0000-01-01T00:00:00Z", "9999-12-31T23:59:59Z");
    let mut aliases = vec![];
    let mut bans = vec![];
    let mut sessions = vec![];
    let mut coverage = vec![];
    for p in &players {
        aliases.extend(repo::list_aliases(&ctx.pool, &p.steam_id).await?.into_iter().map(|a| (p.steam_id.clone(), a)));
        bans.extend(repo::list_bans(&ctx.pool, &p.steam_id).await?);
        sessions.extend(repo::list_sessions(&ctx.pool, &p.steam_id, lo, hi).await?);
        coverage.extend(repo::list_coverage(&ctx.pool, &p.steam_id, lo, hi).await?.into_iter().map(|c| (p.steam_id.clone(), c)));
    }
    let mut squads = vec![];
    for s in repo::list_squads(&ctx.pool).await? {
        let m = repo::list_squad_members(&ctx.pool, s.id).await?;
        squads.push((s, m));
    }
    Ok(ExportBundle {
        format: EXPORT_FORMAT.into(),
        version: EXPORT_VERSION,
        exported_at: fmt_ts(now()),
        mode: if ctx.is_demo() { "demo".into() } else { "live".into() },
        players,
        aliases,
        bans,
        sessions,
        coverage,
        squads,
        servers: repo::list_servers(&ctx.pool, None, false, 100_000).await?,
        events: repo::list_events(&ctx.pool, None, None, false, 100_000, 0).await?,
    })
}

pub fn csv_cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub async fn export(ctx: &Ctx, path: &Path, format: &str) -> AppResult<usize> {
    let b = build_bundle(ctx).await?;
    match format {
        "json" => {
            std::fs::write(path, serde_json::to_vec_pretty(&b).map_err(|e| AppError::Internal { message: e.to_string() })?)?;
            Ok(b.players.len())
        }
        "csv_sessions" => {
            let mut out = String::from("steam_id,provider,kind,server_id,server_name,start_lower,start_upper,end_lower,end_upper,is_open\n");
            for s in &b.sessions {
                let row = [
                    s.steam_id.as_str(),
                    &s.provider,
                    &s.kind,
                    s.server_id.as_deref().unwrap_or(""),
                    s.server_name.as_deref().unwrap_or(""),
                    &s.start_lower,
                    &s.start_upper,
                    &s.end_lower,
                    s.end_upper.as_deref().unwrap_or(""),
                    if s.is_open { "1" } else { "0" },
                ];
                out.push_str(&row.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
                out.push('\n');
            }
            std::fs::write(path, out)?;
            Ok(b.sessions.len())
        }
        "csv_events" => {
            let mut out = String::from("occurred_lower,occurred_upper,approximate,steam_id,persona_name,kind,provider,server\n");
            for e in &b.events {
                let row = [
                    e.occurred_lower.as_str(),
                    &e.occurred_upper,
                    if e.approximate { "1" } else { "0" },
                    e.steam_id.as_deref().unwrap_or(""),
                    e.persona_name.as_deref().unwrap_or(""),
                    &e.kind,
                    &e.provider,
                    e.server_name.as_deref().or(e.server_id.as_deref()).unwrap_or(""),
                ];
                out.push_str(&row.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
                out.push('\n');
            }
            std::fs::write(path, out)?;
            Ok(b.events.len())
        }
        _ => Err(AppError::validation("Неизвестный формат экспорта")),
    }
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub players: usize,
    pub sessions: usize,
    pub skipped_demo: usize,
}

pub async fn import(ctx: &Ctx, path: &Path) -> AppResult<ImportResult> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > 200 * 1024 * 1024 {
        return Err(AppError::validation("Файл больше 200 МБ"));
    }
    let b: ExportBundle = serde_json::from_slice(&std::fs::read(path)?).map_err(|e| AppError::validation(format!("Некорректный файл экспорта: {e}")))?;
    if b.format != EXPORT_FORMAT || b.version > EXPORT_VERSION {
        return Err(AppError::validation("Неподдерживаемый формат или версия файла"));
    }
    let mut res = ImportResult { players: 0, sessions: 0, skipped_demo: 0 };
    let demo_ids: std::collections::HashSet<String> = b.players.iter().filter(|p| p.is_demo).map(|p| p.steam_id.clone()).collect();
    for p in &b.players {
        // Демо и реальные данные не смешиваются.
        if p.is_demo != ctx.is_demo() || crate::domain::ids::SteamId64::parse(&p.steam_id).is_err() {
            res.skipped_demo += 1;
            continue;
        }
        repo::upsert_player(
            &ctx.pool,
            &repo::PlayerUpsert {
                steam_id: p.steam_id.clone(),
                persona_name: p.persona_name.clone(),
                avatar_url: p.avatar_url.clone(),
                profile_url: p.profile_url.clone(),
                visibility: Some(p.visibility.clone()),
                account_created_at: p.account_created_at.clone(),
                country_code: p.country_code.clone(),
                rust_hours: p.rust_hours,
                rust_hours_status: Some(p.rust_hours_status.clone()),
                steam_in_game: p.steam_in_game,
                steam_status: Some(p.steam_status.clone()),
                steam_fetched_at: p.steam_fetched_at.clone(),
                is_demo: p.is_demo,
            },
        )
        .await?;
        res.players += 1;
    }
    let ok = |sid: &str| demo_ids.contains(sid) == ctx.is_demo();
    for (sid, a) in &b.aliases {
        if ok(sid) && repo::get_player(&ctx.pool, sid).await?.is_some() {
            repo::record_alias(&ctx.pool, sid, &a.name, &a.source, crate::infrastructure::clock::parse_ts(&a.last_seen_at).unwrap_or_else(now)).await?;
        }
    }
    for s in &b.servers {
        if s.is_demo == ctx.is_demo() {
            repo::upsert_server(&ctx.pool, s).await?;
        }
    }
    for s in &b.sessions {
        if !ok(&s.steam_id) || repo::get_player(&ctx.pool, &s.steam_id).await?.is_none() {
            continue;
        }
        let p = crate::infrastructure::clock::parse_ts;
        let (Some(sl), Some(su), Some(el)) = (p(&s.start_lower), p(&s.start_upper), p(&s.end_lower)) else { continue };
        let server_ok = match &s.server_id {
            Some(id) => repo::get_server(&ctx.pool, id).await?.is_some(),
            None => true,
        };
        repo::upsert_session(
            &ctx.pool,
            &repo::NewSession {
                steam_id: &s.steam_id,
                provider: &s.provider,
                kind: &s.kind,
                server_id: if server_ok { s.server_id.as_deref() } else { None },
                start_lower: sl,
                start_upper: su,
                end_lower: el,
                end_upper: s.end_upper.as_deref().and_then(p),
                is_open: false,
                external_id: s.external_id.as_deref(),
            },
        )
        .await?;
        res.sessions += 1;
    }
    for (sid, c) in &b.coverage {
        let p = crate::infrastructure::clock::parse_ts;
        if ok(sid)
            && repo::get_player(&ctx.pool, sid).await?.is_some()
            && let (Some(a), Some(e)) = (p(&c.start_at), p(&c.end_at))
        {
            repo::add_coverage(&ctx.pool, sid, &c.provider, a, e).await?;
        }
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::player_service::tests::demo_ctx;

    #[test]
    fn csv_injection_is_neutralized() {
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("plain"), "\"plain\"");
    }

    #[tokio::test]
    async fn export_import_roundtrip_demo() {
        let ctx = demo_ctx().await;
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("e.json");
        let n = export(&ctx, &f, "json").await.unwrap();
        assert!(n > 0);
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(!text.to_lowercase().contains("api_key"));
        // импорт в пустую демо-БД
        let mut ctx2 = demo_ctx().await;
        ctx2.pool = crate::persistence::db::open_memory().await.unwrap();
        let r = import(&ctx2, &f).await.unwrap();
        assert_eq!(r.players, n);
        assert!(r.sessions > 0);
        // в реальный режим демо-данные не импортируются
        let mut ctx3 = demo_ctx().await;
        ctx3.mode = crate::config::Mode::Live;
        ctx3.pool = crate::persistence::db::open_memory().await.unwrap();
        let r = import(&ctx3, &f).await.unwrap();
        assert_eq!(r.players, 0);
        assert_eq!(r.skipped_demo, n);
    }
}
