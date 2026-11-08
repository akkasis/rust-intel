//! Автоматическая привязка профиля Steam к игроку BattleMetrics и друзья Steam.
//!
//! Порядок (от самого надёжного):
//! 1. точное совпадение SteamID (quick-match / match) — редко доступно обычным токенам;
//! 2. сервер из Steam: когда игрок в Rust, Steam сообщает ip:port сервера → сервер на BM → онлайн →
//!    игрок с тем же ником (или ником из истории) — почти точное совпадение;
//! 3. оценка кандидатов по нескольким признакам (ник и история ников, активность, онлайн одновременно
//!    со Steam, общий сервер с последним сервером из Steam). Привязка — только при явном лидере.
//!
//! Каждая автопривязка помечена способом и может быть отменена.

use crate::domain::ids::{BattleMetricsPlayerId, SteamId64};
use crate::error::AppResult;
use crate::infrastructure::clock::{fmt_ts, now, parse_ts};
use crate::persistence::repo;
use crate::providers::NameCandidate;
use crate::services::player_service::{self, Notice};
use crate::state::Ctx;
use chrono::Duration;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub fn norm(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Ник и история ников игрока Steam (нормализованные, без дублей, до 6).
pub async fn steam_names(ctx: &Ctx, sid: &str) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = vec![];
    if let Some(p) = repo::get_player(&ctx.pool, sid).await?
        && let Some(n) = p.persona_name
    {
        out.push(n);
    }
    for a in repo::list_aliases(&ctx.pool, sid).await? {
        out.push(a.name);
    }
    let mut seen = HashSet::new();
    out.retain(|n| n.chars().count() >= 2 && seen.insert(norm(n)));
    out.truncate(6);
    Ok(out)
}

async fn record_attempt(ctx: &Ctx, sid: &str, method: &str, result: &str) {
    let _ = sqlx::query("INSERT OR REPLACE INTO bm_link_attempts(steam_id, attempted_at, method, result) VALUES (?, ?, ?, ?)")
        .bind(sid)
        .bind(fmt_ts(now()))
        .bind(method)
        .bind(result)
        .execute(&ctx.pool)
        .await;
}

async fn recently_attempted(ctx: &Ctx, sid: &str, method: &str, minutes: i64) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bm_link_attempts WHERE steam_id = ? AND method = ? AND attempted_at >= ?")
        .bind(sid)
        .bind(method)
        .bind(fmt_ts(now() - Duration::minutes(minutes)))
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(0)
        > 0
}

pub async fn is_linked(ctx: &Ctx, sid: &str) -> AppResult<bool> {
    Ok(repo::list_links(&ctx.pool, sid).await?.iter().any(|l| l.provider == "battlemetrics"))
}

async fn taken_by_other(ctx: &Ctx, bm: &str, sid: &str) -> AppResult<bool> {
    Ok(sqlx::query_scalar::<_, String>("SELECT steam_id FROM provider_links WHERE provider = 'battlemetrics' AND external_id = ? AND steam_id != ?")
        .bind(bm)
        .bind(sid)
        .fetch_optional(&ctx.pool)
        .await?
        .is_some())
}

/// Способ 2: игрок сейчас в Rust, Steam сообщил сервер → онлайн этого сервера на BM → совпадение ника.
pub async fn try_server_ip_link(ctx: &Ctx, sid: &str, addr: &str, notices: &mut Vec<Notice>) -> AppResult<Option<BattleMetricsPlayerId>> {
    if !ctx.bm.is_configured() || recently_attempted(ctx, sid, "server_ip", 10).await {
        return Ok(None);
    }
    let names: HashSet<String> = steam_names(ctx, sid).await?.iter().map(|n| norm(n)).collect();
    if names.is_empty() {
        return Ok(None);
    }
    let servers = match ctx.bm.servers_by_addr(addr).await {
        Ok(s) => s,
        Err(e) => {
            record_attempt(ctx, sid, "server_ip", &format!("error: {e}")).await;
            return Ok(None);
        }
    };
    let mut hits: Vec<(String, String, String)> = vec![];
    for s in servers.iter().take(2) {
        if let Ok(r) = ctx.bm.server_roster(&s.external_id).await {
            for (id, name) in &r.players {
                if names.contains(&norm(name)) {
                    hits.push((id.clone(), name.clone(), s.name.clone()));
                }
            }
        }
    }
    hits.sort();
    hits.dedup_by(|a, b| a.0 == b.0);
    match hits.as_slice() {
        [(id, name, server)] if !taken_by_other(ctx, id, sid).await? => {
            record_attempt(ctx, sid, "server_ip", &format!("linked {id}")).await;
            notices.push(Notice {
                level: "info", text: format!("BattleMetrics: найден по серверу из Steam — «{name}» сейчас на «{server}» (BM {id})")
            });
            Ok(Some(BattleMetricsPlayerId(id.clone())))
        }
        [] => {
            record_attempt(ctx, sid, "server_ip", &format!("no name match on {addr}")).await;
            Ok(None)
        }
        _ => {
            record_attempt(ctx, sid, "server_ip", &format!("{} name matches", hits.len())).await;
            Ok(None)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkCandidate {
    pub bm_id: String,
    pub name: String,
    pub score: i32,
    pub reasons: Vec<String>,
    pub last_seen: Option<String>,
    pub online_now: Option<String>,
    pub source_url: String,
}

/// Способ 3: кандидаты по нику и истории ников, с оценкой.
pub async fn link_candidates(ctx: &Ctx, sid: &str, max_detailed: usize) -> AppResult<Vec<LinkCandidate>> {
    let names = steam_names(ctx, sid).await?;
    let normed: HashSet<String> = names.iter().map(|n| norm(n)).collect();
    let player = repo::get_player(&ctx.pool, sid).await?;
    let current = player.as_ref().and_then(|p| p.persona_name.clone()).map(|n| norm(&n));
    let in_rust = player.as_ref().and_then(|p| p.steam_in_game);
    let public = player.as_ref().is_some_and(|p| p.visibility == "public");
    let last_server = player.as_ref().and_then(|p| p.last_game_server.clone());
    let last_server_at = player.as_ref().and_then(|p| p.last_game_server_at.as_deref().and_then(parse_ts));
    let mut found: HashMap<String, NameCandidate> = HashMap::new();
    for n in names.iter().take(4) {
        match ctx.bm.search_by_name(n).await {
            Ok(list) => {
                for c in list {
                    found.entry(c.external_id.clone()).or_insert(c);
                }
            }
            Err(e) => tracing::warn!(error = %e, "BM поиск по нику"),
        }
    }
    let mut cands: Vec<NameCandidate> = found.into_values().filter(|c| normed.contains(&norm(&c.name))).collect();
    cands.sort_by_key(|c| std::cmp::Reverse(c.last_seen));
    let t = now();
    let mut out = vec![];
    for (i, c) in cands.into_iter().enumerate() {
        let mut score = 0;
        let mut reasons = vec![];
        if current.as_deref() == Some(norm(&c.name).as_str()) {
            score += 3;
            reasons.push("совпадает текущий ник Steam".to_string());
        } else {
            score += 1;
            reasons.push("совпадает один из прошлых ников Steam".to_string());
        }
        if let Some(ls) = c.last_seen {
            if t - ls <= Duration::days(1) {
                score += 2;
                reasons.push("активен за последние сутки".into());
            } else if t - ls <= Duration::days(7) {
                score += 1;
                reasons.push("активен за неделю".into());
            } else if t - ls > Duration::days(60) {
                score -= 2;
                reasons.push("давно не активен".into());
            }
        }
        let mut online_now = None;
        if i < max_detailed
            && let Ok(info) = ctx.bm.player_info(&BattleMetricsPlayerId(c.external_id.clone())).await
        {
            let hist: HashSet<String> = info.names.iter().map(|n| norm(n)).collect();
            let shared = normed.iter().filter(|n| hist.contains(*n)).count();
            if shared >= 2 {
                let add = ((shared - 1) * 2).min(6) as i32;
                score += add;
                reasons.push(format!("совпадает история ников: {shared}"));
            }
            let on = info.servers.iter().find(|s| s.online);
            online_now = on.map(|s| s.server_name.clone());
            match (on.is_some(), in_rust) {
                (true, Some(true)) => {
                    score += 4;
                    reasons.push("онлайн на BM одновременно с «в Rust» в Steam".into());
                }
                (true, Some(false)) if public => {
                    score -= 3;
                    reasons.push("онлайн на BM, а Steam говорит «не в игре»".into());
                }
                (false, Some(true)) => score -= 1,
                _ => {}
            }
            if let (Some(addr), Some(at)) = (&last_server, last_server_at)
                && info.servers.iter().any(|s| s.address.as_deref() == Some(addr.as_str()) && s.last_seen.is_some_and(|l| (l - at).num_minutes().abs() <= 180))
            {
                score += 5;
                reasons.push("был на том же сервере, что сообщал Steam".into());
            }
        }
        out.push(LinkCandidate {
            source_url: format!("https://www.battlemetrics.com/players/{}", c.external_id),
            bm_id: c.external_id,
            name: c.name,
            score,
            reasons,
            last_seen: c.last_seen.map(fmt_ts),
            online_now,
        });
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.score));
    Ok(out)
}

/// Полная автопривязка. Возвращает id, если удалось связать.
pub async fn auto_link(ctx: &Ctx, id: &SteamId64, notices: &mut Vec<Notice>) -> AppResult<Option<BattleMetricsPlayerId>> {
    let sid = id.as_str();
    // 1. точный SteamID
    match ctx.bm.match_steam_id(id).await {
        Ok(Some(bm)) => {
            notices.push(Notice { level: "info", text: format!("BattleMetrics: найден точно по SteamID ({})", bm.0) });
            player_service::save_link(ctx, sid, &bm, "exact_identifier").await?;
            return Ok(Some(bm));
        }
        Ok(None) => {}
        Err(e) => {
            player_service::record_err(ctx, "battlemetrics", &e).await;
            notices.push(Notice { level: "warning", text: format!("BattleMetrics: {e}") });
            return Ok(None);
        }
    }
    // 2. сервер из Steam
    let addr = repo::get_player(&ctx.pool, sid).await?.and_then(|p| {
        let at = p.last_game_server_at.as_deref().and_then(parse_ts)?;
        (now() - at <= Duration::minutes(15)).then_some(p.last_game_server).flatten()
    });
    if let Some(addr) = addr
        && let Some(bm) = try_server_ip_link(ctx, sid, &addr, notices).await?
    {
        player_service::save_link(ctx, sid, &bm, "server_ip_match").await?;
        return Ok(Some(bm));
    }
    // 3. оценка кандидатов
    let cands = link_candidates(ctx, sid, 5).await?;
    record_attempt(ctx, sid, "scored", &format!("{} candidates", cands.len())).await;
    let best = cands.first();
    let second = cands.get(1).map(|c| c.score).unwrap_or(i32::MIN);
    match best {
        Some(b) if b.score >= 5 && b.score - second >= 3 && !taken_by_other(ctx, &b.bm_id, sid).await? => {
            let bm = BattleMetricsPlayerId(b.bm_id.clone());
            player_service::save_link(ctx, sid, &bm, "scored_match").await?;
            notices.push(Notice {
                level: "warning",
                text: format!("BattleMetrics: связан автоматически с «{}» (BM {}) — {}. Проверьте в карточке.", b.name, b.bm_id, b.reasons.join(", ")),
            });
            Ok(Some(bm))
        }
        Some(_) => {
            notices.push(Notice {
                level: "warning",
                text: format!(
                    "BattleMetrics: {} кандидат(ов) без явного лидера. Откройте «Привязать BattleMetrics» — там они отсортированы по совпадениям. Программа также попробует привязать автоматически, когда игрок зайдёт в Rust.",
                    cands.len()
                ),
            });
            Ok(None)
        }
        None => {
            notices.push(Notice {
                level: "warning",
                text: "BattleMetrics: по нику и истории ников не найден. Программа привяжет автоматически, когда игрок зайдёт в Rust (по серверу из Steam), если профиль открыт.".into(),
            });
            Ok(None)
        }
    }
}

// ------------------------------------------------------------------ друзья Steam

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FriendRow {
    pub friend_id: String,
    pub persona_name: Option<String>,
    pub avatar_url: Option<String>,
    pub visibility: Option<String>,
    pub in_rust: Option<bool>,
    pub friend_since: Option<String>,
    pub fetched_at: String,
}

/// Загружает список друзей (если открыт) и их публичные профили. Не чаще раза в 6 часов.
pub async fn refresh_friends(ctx: &Ctx, id: &SteamId64, force: bool, notices: &mut Vec<Notice>) -> AppResult<()> {
    let sid = id.as_str();
    let p = repo::get_player(&ctx.pool, sid).await?;
    let fresh = p.as_ref().and_then(|p| p.friends_fetched_at.as_deref()).and_then(parse_ts).is_some_and(|f| now() - f < Duration::hours(6));
    if fresh && !force {
        return Ok(());
    }
    let t = fmt_ts(now());
    let list = match ctx.steam.friends(id).await {
        Ok(Some(l)) => l,
        Ok(None) => {
            sqlx::query("UPDATE players SET friends_status = 'private', friends_fetched_at = ? WHERE steam_id = ?")
                .bind(&t)
                .bind(sid)
                .execute(&ctx.pool)
                .await?;
            notices.push(Notice {
                level: "info", text: "Список друзей Steam скрыт — тиммейты будут искаться только по совместной игре.".into()
            });
            return Ok(());
        }
        Err(e) => {
            sqlx::query("UPDATE players SET friends_status = 'error', friends_fetched_at = ? WHERE steam_id = ?").bind(&t).bind(sid).execute(&ctx.pool).await?;
            notices.push(Notice { level: "warning", text: format!("Друзья Steam: {e}") });
            return Ok(());
        }
    };
    let ids: Vec<SteamId64> = list.iter().take(300).map(|f| f.steam_id.clone()).collect();
    let summaries = use_summaries(ctx, &ids).await;
    let mut tx = ctx.pool.begin().await?;
    sqlx::query("DELETE FROM steam_friends WHERE steam_id = ?").bind(sid).execute(&mut *tx).await?;
    for f in list.iter().take(300) {
        let s = summaries.get(f.steam_id.as_str());
        sqlx::query(
            "INSERT OR REPLACE INTO steam_friends(steam_id, friend_id, persona_name, avatar_url, visibility, in_rust, friend_since, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(sid)
        .bind(f.steam_id.as_str())
        .bind(s.and_then(|s| s.persona_name.clone()))
        .bind(s.and_then(|s| s.avatar_url.clone()))
        .bind(s.map(|s| s.visibility.clone()))
        .bind(s.and_then(|s| s.in_rust))
        .bind(f.friend_since.map(fmt_ts))
        .bind(&t)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE players SET friends_status = 'ok', friends_fetched_at = ? WHERE steam_id = ?").bind(&t).bind(sid).execute(&mut *tx).await?;
    tx.commit().await?;
    notices.push(Notice { level: "info", text: format!("Друзья Steam: {}", list.len()) });
    Ok(())
}

async fn use_summaries(ctx: &Ctx, ids: &[SteamId64]) -> HashMap<String, crate::providers::PlayerSummary> {
    use crate::providers::ProfileSource;
    match ctx.steam.summaries(ids).await {
        Ok(v) => v.into_iter().map(|s| (s.steam_id.to_string(), s)).collect(),
        Err(_) => HashMap::new(),
    }
}

pub async fn list_friends(ctx: &Ctx, sid: &str) -> AppResult<Vec<FriendRow>> {
    Ok(sqlx::query_as(
        "SELECT friend_id, persona_name, avatar_url, visibility, in_rust, friend_since, fetched_at FROM steam_friends WHERE steam_id = ? ORDER BY in_rust DESC, persona_name",
    )
    .bind(sid)
    .fetch_all(&ctx.pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_normalization() {
        assert_eq!(norm("  Raven   The  Bird "), "raven the bird");
        assert_eq!(norm("ВОРОН"), "ворон");
    }
}
