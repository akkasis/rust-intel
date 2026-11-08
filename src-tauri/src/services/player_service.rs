//! Поиск, профиль, обновление игрока из источников.

use crate::domain::bans::{BanKind, BanRecord, RiskInput, RiskSummary, compute_risk};
use crate::domain::ids::{BattleMetricsPlayerId, ParsedInput, SteamId64, parse_input};
use crate::domain::provenance::{DataStatus, Provenance};
use crate::error::{AppError, AppResult};
use crate::infrastructure::clock::{fmt_ts, now, parse_ts};
use crate::persistence::models::*;
use crate::persistence::repo::{self, PlayerUpsert};
use crate::providers::error::ProviderError;
use crate::providers::{NameCandidate, ProfileSource, demo};
use crate::state::Ctx;
use chrono::Duration;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct SearchCandidate {
    pub steam_id: Option<String>,
    pub name: String,
    pub provider: String,
    pub external_id: Option<String>,
    pub source_url: Option<String>,
    pub last_seen: Option<String>,
    pub note: String,
    pub is_demo: bool,
}

#[derive(Debug, Serialize)]
pub struct Notice {
    pub level: &'static str,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct SearchOutcome {
    pub parsed: Option<ParsedInput>,
    pub resolved_steam_id: Option<String>,
    pub candidates: Vec<SearchCandidate>,
    pub notices: Vec<Notice>,
}

pub async fn search(ctx: &Ctx, input: &str) -> AppResult<SearchOutcome> {
    let parsed = parse_input(input)?;
    let mut out = SearchOutcome { parsed: Some(parsed.clone()), resolved_steam_id: None, candidates: vec![], notices: vec![] };
    match parsed {
        ParsedInput::SteamId { steam_id, .. } => {
            if ctx.is_demo() && !demo::is_demo_id(&steam_id) {
                return Err(AppError::not_found("В демо-режиме доступны только вымышленные профили. Переключитесь в реальный режим в «Настройках»."));
            }
            refresh_player(ctx, &steam_id, &mut out.notices).await?;
            repo::add_search_history(&ctx.pool, input.trim(), Some(steam_id.as_str())).await?;
            out.resolved_steam_id = Some(steam_id.to_string());
        }
        ParsedInput::Vanity { vanity } => {
            if ctx.is_demo() {
                return Err(AppError::not_found("Vanity URL разрешается через Steam Web API — доступно в реальном режиме."));
            }
            let id = ctx.steam.resolve_vanity(&vanity).await.map_err(|e| AppError::provider("steam", e))?;
            refresh_player(ctx, &id, &mut out.notices).await?;
            repo::add_search_history(&ctx.pool, input.trim(), Some(id.as_str())).await?;
            out.notices.push(Notice { level: "info", text: format!("Vanity «{vanity}» разрешён Steam Web API в {id}") });
            out.resolved_steam_id = Some(id.to_string());
        }
        ParsedInput::Ambiguous { text } => {
            // Локальные (кэшированные) совпадения по нику
            for p in repo::list_players(&ctx.pool, Some(&text), false, 10, 0).await? {
                out.candidates.push(SearchCandidate {
                    steam_id: Some(p.steam_id.clone()),
                    name: p.persona_name.clone().unwrap_or_else(|| p.steam_id.clone()),
                    provider: if p.is_demo { "demo".into() } else { "local_cache".into() },
                    external_id: None,
                    source_url: p.profile_url.clone(),
                    last_seen: p.steam_fetched_at.clone(),
                    note: "Из локальной базы (ранее найденный профиль)".into(),
                    is_demo: p.is_demo,
                });
            }
            if !ctx.is_demo() {
                if ctx.bm.is_configured() {
                    match ctx.bm.search_by_name(&text).await {
                        Ok(list) => out.candidates.extend(list.into_iter().map(bm_candidate)),
                        Err(e) => out.notices.push(Notice { level: "warning", text: format!("BattleMetrics: {e}") }),
                    }
                } else {
                    out.notices.push(Notice {
                        level: "info",
                        text: "Поиск по нику требует источника с такой возможностью (BattleMetrics.com с подпиской). Steam Web API по нику не ищет.".into(),
                    });
                }
                if text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') && (2..=32).contains(&text.len()) {
                    out.notices.push(Notice {
                        level: "hint", text: format!("Если «{text}» — это vanity URL профиля, найдите его как id/{text}")
                    });
                }
            }
            repo::add_search_history(&ctx.pool, input.trim(), None).await?;
            if out.candidates.is_empty() {
                out.notices.push(Notice {
                    level: "info", text: "Совпадений нет. Вставьте SteamID64 или ссылку на профиль Steam.".into()
                });
            }
        }
    }
    Ok(out)
}

fn bm_candidate(c: NameCandidate) -> SearchCandidate {
    SearchCandidate {
        steam_id: c.steam_id.map(|s| s.to_string()),
        name: c.name,
        provider: c.provider.into(),
        external_id: Some(c.external_id),
        source_url: c.source_url,
        last_seen: c.last_seen.map(fmt_ts),
        note: "Кандидат BattleMetrics: SteamID не подтверждён — откройте источник и проверьте вручную".into(),
        is_demo: false,
    }
}

/// Обновляет данные игрока из настроенных источников; при отсутствии ключей сохраняет заглушку
/// со статусом not_configured (не выдумывает данные).
pub async fn refresh_player(ctx: &Ctx, id: &SteamId64, notices: &mut Vec<Notice>) -> AppResult<()> {
    let sid = id.as_str();
    if ctx.is_demo() {
        if repo::get_player(&ctx.pool, sid).await?.is_none() {
            return Err(AppError::not_found("Вымышленный профиль не найден"));
        }
        return Ok(());
    }
    repo::ensure_player(&ctx.pool, sid, false).await?;
    if !ctx.steam.is_configured() {
        repo::upsert_player(
            &ctx.pool,
            &PlayerUpsert { steam_id: sid.into(), profile_url: Some(id.profile_url()), steam_status: Some("not_configured".into()), ..Default::default() },
        )
        .await?;
        notices.push(Notice {
            level: "warning", text: "Steam Web API не настроен: профиль сохранён только по SteamID. Добавьте ключ в «Интеграциях».".into()
        });
        return Ok(());
    }
    let t = now();
    let prev = repo::get_player(&ctx.pool, sid).await?;
    match ctx.steam.summaries(std::slice::from_ref(id)).await {
        Ok(list) => {
            repo::record_health_ok(&ctx.pool, "steam").await?;
            let Some(s) = list.into_iter().next() else {
                return Err(AppError::not_found("Steam не вернул профиль для этого SteamID"));
            };
            if let Some(addr) = &s.game_server_ip {
                sqlx::query("UPDATE players SET last_game_server = ?, last_game_server_at = ? WHERE steam_id = ?")
                    .bind(addr)
                    .bind(fmt_ts(t))
                    .bind(sid)
                    .execute(&ctx.pool)
                    .await?;
            }
            if let Some(name) = &s.persona_name {
                repo::record_alias(&ctx.pool, sid, name, "steam", t).await?;
            }
            let renamed = prev.as_ref().and_then(|p| p.persona_name.clone()).filter(|old| Some(old) != s.persona_name.as_ref());
            repo::upsert_player(
                &ctx.pool,
                &PlayerUpsert {
                    steam_id: sid.into(),
                    persona_name: s.persona_name.clone(),
                    avatar_url: s.avatar_url.clone(),
                    profile_url: s.profile_url.clone(),
                    visibility: Some(s.visibility.clone()),
                    account_created_at: s.created_at.map(fmt_ts),
                    country_code: s.country_code.clone(),
                    steam_in_game: s.in_rust,
                    steam_status: Some("verified".into()),
                    steam_fetched_at: Some(fmt_ts(t)),
                    ..Default::default()
                },
            )
            .await?;
            if let (Some(old), Some(new)) = (renamed, s.persona_name.clone()) {
                repo::insert_event(
                    &ctx.pool,
                    &repo::NewEvent {
                        steam_id: Some(sid),
                        provider: "steam",
                        kind: "name_changed",
                        dedup_key: format!("steam:name:{sid}:{new}:{}", fmt_ts(t)),
                        occurred_lower: prev.as_ref().and_then(|p| p.steam_fetched_at.as_deref()).and_then(parse_ts).unwrap_or(t),
                        occurred_upper: t,
                        approximate: true,
                        server_id: None,
                        details: serde_json::json!({"from": old, "to": new}),
                    },
                )
                .await?;
            }
        }
        Err(e) => {
            record_err(ctx, "steam", &e).await;
            repo::upsert_player(&ctx.pool, &PlayerUpsert { steam_id: sid.into(), steam_status: Some("error".into()), ..Default::default() }).await?;
            notices.push(Notice { level: "error", text: format!("Steam профиль: {e}. Показаны кэшированные данные.") });
            return Ok(());
        }
    }
    match ctx.steam.bans(std::slice::from_ref(id)).await {
        Ok(list) => {
            if let Some((_, bans)) = list.into_iter().next() {
                let old = repo::list_bans(&ctx.pool, sid).await?;
                let changed = ban_signature(&old) != ban_signature_records(&bans) && !old.is_empty();
                repo::replace_bans(&ctx.pool, sid, "steam", &bans).await?;
                if changed {
                    repo::insert_event(
                        &ctx.pool,
                        &repo::NewEvent {
                            steam_id: Some(sid),
                            provider: "steam",
                            kind: "ban_change",
                            dedup_key: format!("steam:ban:{sid}:{}", ban_signature_records(&bans)),
                            occurred_lower: t,
                            occurred_upper: t,
                            approximate: true,
                            server_id: None,
                            details: serde_json::json!({"signature": ban_signature_records(&bans)}),
                        },
                    )
                    .await?;
                }
            }
        }
        Err(e) => notices.push(Notice { level: "warning", text: format!("Баны Steam: {e}") }),
    }
    match ctx.steam.rust_hours(id).await {
        Ok(h) => {
            repo::upsert_player(
                &ctx.pool,
                &PlayerUpsert {
                    steam_id: sid.into(),
                    rust_hours: h,
                    rust_hours_status: Some(if h.is_some() { "verified" } else { "hidden" }.into()),
                    ..Default::default()
                },
            )
            .await?
        }
        Err(e) => notices.push(Notice { level: "warning", text: format!("Часы в Rust: {e}") }),
    }
    // Друзья Steam — для поиска настоящих тиммейтов.
    crate::services::linking::refresh_friends(ctx, id, false, notices).await?;
    // BattleMetrics: уже привязан — обновляем; нет — автопривязка (SteamID → сервер из Steam → оценка кандидатов).
    if ctx.bm.is_configured() {
        let existing = repo::list_links(&ctx.pool, sid).await?.into_iter().find(|l| l.provider == "battlemetrics");
        match existing {
            Some(l) => sync_bm(ctx, id, &BattleMetricsPlayerId(l.external_id), notices).await?,
            None => {
                if let Some(bm) = crate::services::linking::auto_link(ctx, id, notices).await? {
                    sync_bm(ctx, id, &bm, notices).await?;
                }
            }
        }
    }
    Ok(())
}

pub async fn save_link(ctx: &Ctx, sid: &str, bm: &BattleMetricsPlayerId, evidence: &str) -> AppResult<()> {
    crate::persistence::bm_repo::upsert_player(&ctx.pool, &bm.0, None, None, false).await?;
    crate::persistence::bm_repo::set_steam(&ctx.pool, &bm.0, Some(sid)).await?;
    repo::upsert_link(
        &ctx.pool,
        &ProviderLinkRow {
            steam_id: sid.into(),
            provider: "battlemetrics".into(),
            external_id: bm.0.clone(),
            match_evidence: evidence.into(),
            confidence: if matches!(evidence, "exact_identifier" | "server_ip_match") { "high".into() } else { "medium".into() },
            source_url: Some(format!("https://www.battlemetrics.com/players/{}", bm.0)),
            linked_at: fmt_ts(now()),
        },
    )
    .await?;
    Ok(())
}

/// Загружает из BattleMetrics всё о привязанном игроке: серверы, время, ники, сессии (в слой BM и в профиль Steam),
/// и при включённой настройке — включает наблюдение за его недавними серверами (для поиска тиммейтов).
pub async fn sync_bm(ctx: &Ctx, id: &SteamId64, bm: &BattleMetricsPlayerId, notices: &mut Vec<Notice>) -> AppResult<()> {
    let sid = id.as_str();
    if let Err(e) = crate::services::bm_service::refresh_player(ctx, bm, notices).await {
        notices.push(Notice { level: "error", text: format!("BattleMetrics, профиль игрока: {e}") });
        return Ok(());
    }
    crate::persistence::bm_repo::set_steam(&ctx.pool, &bm.0, Some(sid)).await?;
    let t = fmt_ts(now());
    // серверы → профиль Steam
    sqlx::query(
        "INSERT OR REPLACE INTO player_server_stats(steam_id, server_id, provider, server_name, online, first_seen, last_seen, time_played_secs, fetched_at)
         SELECT ?, server_id, 'battlemetrics', server_name, online, first_seen, last_seen, time_played_secs, ? FROM bm_player_servers WHERE bm_id = ?",
    )
    .bind(sid)
    .bind(&t)
    .bind(&bm.0)
    .execute(&ctx.pool)
    .await?;
    // история сессий BM → сессии профиля Steam
    sqlx::query(
        "INSERT OR IGNORE INTO sessions(steam_id, provider, kind, server_id, start_lower, start_upper, end_lower, end_upper, is_open, external_id)
         SELECT ?, 'battlemetrics', 'provider_reported', server_id, start_lower, start_upper, end_lower, end_upper, is_open, external_id
         FROM bm_sessions WHERE bm_id = ? AND source = 'bm_api'",
    )
    .bind(sid)
    .bind(&bm.0)
    .execute(&ctx.pool)
    .await?;
    let oldest: Option<String> =
        sqlx::query_scalar("SELECT MIN(start_lower) FROM bm_sessions WHERE bm_id = ? AND source = 'bm_api'").bind(&bm.0).fetch_one(&ctx.pool).await?;
    if let Some(o) = oldest.as_deref().and_then(parse_ts) {
        repo::add_coverage(&ctx.pool, sid, "battlemetrics", o, now()).await?;
    }
    for n in crate::persistence::bm_repo::list_names(&ctx.pool, &bm.0).await? {
        repo::record_alias(&ctx.pool, sid, &n.name, "battlemetrics", parse_ts(&n.last_seen).unwrap_or_else(now)).await?;
    }
    auto_watch(ctx, &bm.0, notices).await
}

/// Включает наблюдение за серверами, где игрок был за последние 7 дней (до 3 серверов, общий лимит 25).
async fn auto_watch(ctx: &Ctx, bm_id: &str, notices: &mut Vec<Notice>) -> AppResult<()> {
    if !ctx.settings.auto_watch_servers {
        return Ok(());
    }
    let since = fmt_ts(now() - Duration::days(7));
    let recent: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT server_id, server_name FROM bm_player_servers WHERE bm_id = ? AND (online = 1 OR last_seen >= ?) AND server_id LIKE 'battlemetrics:%'
         ORDER BY online DESC, last_seen DESC LIMIT 3",
    )
    .bind(bm_id)
    .bind(&since)
    .fetch_all(&ctx.pool)
    .await?;
    let mut added = vec![];
    for (server, name) in recent {
        let watch = crate::persistence::bm_repo::list_watch(&ctx.pool).await?;
        if watch.iter().any(|w| w.server_id == server && w.enabled) {
            continue;
        }
        if watch.iter().filter(|w| w.enabled).count() >= 25 {
            notices
                .push(Notice { level: "warning", text: "Лимит 25 наблюдаемых серверов — новые не добавлены".into() });
            break;
        }
        crate::persistence::bm_repo::set_watch(&ctx.pool, &server, true).await?;
        added.push(name.unwrap_or(server));
    }
    if !added.is_empty() {
        notices.push(Notice {
            level: "info",
            text: format!("Включено наблюдение за серверами: {} — тиммейты будут находиться, пока программа запущена", added.join(", ")),
        });
    }
    Ok(())
}

/// Ручная привязка игрока BattleMetrics (по ID или ссылке) к профилю Steam.
pub async fn link_bm(ctx: &Ctx, steam_id: &str, input: &str) -> AppResult<Vec<Notice>> {
    if ctx.is_demo() {
        return Err(AppError::validation("В демо-режиме BattleMetrics не используется"));
    }
    let id = SteamId64::parse(steam_id)?;
    ensure_known_player(ctx, steam_id).await?;
    let bm = crate::providers::battlemetrics::parse_player_id(input)
        .ok_or_else(|| AppError::validation("Нужен ID игрока BattleMetrics или ссылка вида battlemetrics.com/players/123456"))?;
    let info = ctx.bm.player_info(&bm).await.map_err(|e| AppError::provider("battlemetrics", e))?;
    let evidence = match &info.steam_id {
        Some(s) if s != steam_id => {
            return Err(AppError::validation(format!("Этот игрок BattleMetrics принадлежит другому SteamID ({s}). Привязка отклонена.")));
        }
        Some(_) => "exact_identifier",
        None => "user_confirmed",
    };
    if let Some(other) =
        sqlx::query_scalar::<_, String>("SELECT steam_id FROM provider_links WHERE provider = 'battlemetrics' AND external_id = ? AND steam_id != ?")
            .bind(&bm.0)
            .bind(steam_id)
            .fetch_optional(&ctx.pool)
            .await?
    {
        return Err(AppError::validation(format!("Этот игрок BattleMetrics уже привязан к профилю {other}. Сначала отвяжите его там.")));
    }
    save_link(ctx, steam_id, &bm, evidence).await?;
    let mut notices =
        vec![Notice { level: "info", text: format!("Привязан игрок BattleMetrics «{}» ({})", info.name.clone().unwrap_or_default(), bm.0) }];
    sync_bm(ctx, &id, &bm, &mut notices).await?;
    Ok(notices)
}

pub async fn unlink_bm(ctx: &Ctx, steam_id: &str) -> AppResult<()> {
    SteamId64::parse(steam_id)?;
    sqlx::query("UPDATE bm_players SET steam_id = NULL WHERE steam_id = ?").bind(steam_id).execute(&ctx.pool).await?;
    sqlx::query("DELETE FROM provider_links WHERE steam_id = ? AND provider = 'battlemetrics'").bind(steam_id).execute(&ctx.pool).await?;
    sqlx::query("DELETE FROM player_server_stats WHERE steam_id = ? AND provider = 'battlemetrics'").bind(steam_id).execute(&ctx.pool).await?;
    sqlx::query("DELETE FROM sessions WHERE steam_id = ? AND provider = 'battlemetrics'").bind(steam_id).execute(&ctx.pool).await?;
    sqlx::query("DELETE FROM coverage_windows WHERE steam_id = ? AND provider = 'battlemetrics'").bind(steam_id).execute(&ctx.pool).await?;
    Ok(())
}

pub async fn search_bm(ctx: &Ctx, name: &str) -> AppResult<Vec<SearchCandidate>> {
    if ctx.is_demo() {
        return Err(AppError::validation("В демо-режиме BattleMetrics не используется"));
    }
    let name = name.trim();
    if name.chars().count() < 2 || name.len() > 64 {
        return Err(AppError::validation("Ник: от 2 до 64 символов"));
    }
    let list = ctx.bm.search_by_name(name).await.map_err(|e| AppError::provider("battlemetrics", e))?;
    Ok(list.into_iter().map(bm_candidate).collect())
}

pub async fn record_err(ctx: &Ctx, provider: &str, e: &ProviderError) {
    let status = match e {
        ProviderError::NotConfigured { .. } => "not_configured",
        ProviderError::Forbidden { .. } => "down",
        ProviderError::Unsupported { .. } => "unsupported",
        _ => "degraded",
    };
    let _ = repo::record_health_error(&ctx.pool, provider, status, e.kind(), &e.to_string()).await;
}

fn ban_signature(rows: &[BanRow]) -> String {
    let mut v: Vec<String> = rows.iter().map(|b| format!("{}:{}:{}", b.kind, b.count.unwrap_or(0), b.active)).collect();
    v.sort();
    v.join("|")
}
fn ban_signature_records(rows: &[BanRecord]) -> String {
    let mut v: Vec<String> = rows.iter().map(|b| format!("{}:{}:{}", b.kind.as_str(), b.count.unwrap_or(0), b.active)).collect();
    v.sort();
    v.join("|")
}

// ---------------------------------------------------------------- профиль

#[derive(Debug, Serialize)]
pub struct LastSeen {
    pub at: String,
    /// observed_server | steam_in_game
    pub scope: String,
    pub provider: String,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub open: bool,
    pub approximate: bool,
}

#[derive(Debug, Serialize)]
pub struct ObservedServer {
    pub server_id: String,
    pub server_name: Option<String>,
    pub sessions: i64,
    pub online_secs: i64,
    pub last_at: String,
}

#[derive(Debug, Serialize)]
pub struct Connection {
    /// teammate | friend | friend_teammate
    pub kind: String,
    pub steam_id: Option<String>,
    pub bm_id: Option<String>,
    pub name: Option<String>,
    pub steam_friend: bool,
    pub friend_since: Option<String>,
    pub in_rust_now: Option<bool>,
    pub joined_together: u32,
    pub left_together: u32,
    pub co_sessions: u32,
    pub overlap_secs: i64,
    pub last_together: Option<String>,
    pub confidence: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub in_same_squad: bool,
}

#[derive(Debug, Serialize)]
pub struct ProfileProvenance {
    pub profile: Provenance,
    pub bans: Provenance,
    pub hours: Provenance,
    pub sessions: Provenance,
    pub combat: Provenance,
}

#[derive(Debug, Serialize)]
pub struct PlayerProfile {
    pub player: PlayerRow,
    pub steam2: String,
    pub steam3: String,
    pub account_id: u32,
    pub provenance: ProfileProvenance,
    pub aliases: Vec<AliasRow>,
    pub bans: Vec<BanRow>,
    pub links: Vec<ProviderLinkRow>,
    pub sessions_total: i64,
    pub first_session_at: Option<String>,
    pub last_session_at: Option<String>,
    pub last_seen: Option<LastSeen>,
    pub servers: Vec<ObservedServer>,
    pub server_stats: Vec<ServerStatRow>,
    pub recent_sessions: Vec<SessionRow>,
    pub squads: Vec<SquadRow>,
    pub tracked: Option<TrackedRow>,
    pub risk: RiskSummary,
    pub connections: Vec<Connection>,
}

pub fn ban_rows_to_records(rows: &[BanRow]) -> Vec<BanRecord> {
    rows.iter()
        .filter_map(|b| {
            Some(BanRecord {
                kind: BanKind::parse(&b.kind)?,
                provider: b.provider.clone(),
                count: b.count,
                active: b.active,
                days_since_last: b.days_since_last,
                reason: b.reason.clone(),
                project_name: (!b.project_name.is_empty()).then(|| b.project_name.clone()),
                revoked: b.revoked,
                source_url: b.source_url.clone(),
                observed_at: parse_ts(&b.observed_at).unwrap_or_else(now),
            })
        })
        .collect()
}

pub fn risk_for(ctx: &Ctx, p: &PlayerRow, bans: &[BanRow]) -> RiskSummary {
    let recs = ban_rows_to_records(bans);
    let age = p.account_created_at.as_deref().and_then(parse_ts).map(|c| (now() - c).num_days());
    compute_risk(
        &RiskInput {
            bans: &recs,
            account_age_days: age,
            rust_hours: p.rust_hours,
            profile_public: match p.visibility.as_str() {
                "public" => Some(true),
                "private" | "friends_only" => Some(false),
                _ => None,
            },
        },
        ctx.settings.risk_indicators,
    )
}

pub async fn get_profile(ctx: &Ctx, steam_id: &str) -> AppResult<PlayerProfile> {
    let id = SteamId64::parse(steam_id)?;
    let player = repo::get_player(&ctx.pool, steam_id).await?.ok_or_else(|| AppError::not_found("Игрок не найден в локальной базе — выполните поиск"))?;
    let t = now();
    let max_age = Duration::hours(24);
    let src = if player.is_demo { "demo" } else { "steam" };
    let profile_status = if player.is_demo { DataStatus::Demo } else { DataStatus::parse(&player.steam_status) };
    let fetched = player.steam_fetched_at.as_deref().and_then(parse_ts);
    let mk = |status: DataStatus| {
        let mut p = Provenance::new(src, status);
        p.fetched_at = fetched;
        p.source_url = if player.is_demo { None } else { Some(id.profile_url()) };
        p.with_staleness(t, max_age)
    };
    let bans = repo::list_bans(&ctx.pool, steam_id).await?;
    let hours_status = match (player.is_demo, player.rust_hours_status.as_str()) {
        (true, _) => DataStatus::Demo,
        (_, "verified") => DataStatus::Verified,
        (_, "hidden") => DataStatus::Partial,
        _ => profile_status,
    };
    let links = repo::list_links(&ctx.pool, steam_id).await?;
    let (first, last, total) = repo::session_span(&ctx.pool, steam_id).await?;
    let from = fmt_ts(t - Duration::days(400));
    let all_sessions = repo::list_sessions(&ctx.pool, steam_id, &from, &fmt_ts(t)).await?;
    let mut servers: BTreeMap<String, ObservedServer> = BTreeMap::new();
    for s in &all_sessions {
        let Some(sv) = &s.server_id else { continue };
        let secs = match (parse_ts(&s.start_upper), parse_ts(&s.end_lower)) {
            (Some(a), Some(b)) if b > a => (b - a).num_seconds(),
            _ => 0,
        };
        let e = servers.entry(sv.clone()).or_insert(ObservedServer {
            server_id: sv.clone(),
            server_name: s.server_name.clone(),
            sessions: 0,
            online_secs: 0,
            last_at: s.end_lower.clone(),
        });
        e.sessions += 1;
        e.online_secs += secs;
        if s.end_lower > e.last_at {
            e.last_at = s.end_lower.clone();
        }
    }
    let mut servers: Vec<ObservedServer> = servers.into_values().collect();
    servers.sort_by(|a, b| b.last_at.cmp(&a.last_at));
    let server_stats = repo::list_server_stats(&ctx.pool, steam_id).await?;
    let mut last_seen = all_sessions.first().map(|s| LastSeen {
        at: s.end_lower.clone(),
        scope: if s.server_id.is_some() { "observed_server".into() } else { "steam_in_game".into() },
        provider: s.provider.clone(),
        server_id: s.server_id.clone(),
        server_name: s.server_name.clone(),
        open: s.is_open,
        approximate: s.kind == "inferred_from_polls",
    });
    // Статистика BattleMetrics по серверам может быть свежее сессий.
    if let Some(st) = server_stats.iter().filter(|x| x.last_seen.is_some()).max_by(|a, b| (a.online, &a.last_seen).cmp(&(b.online, &b.last_seen)))
        && (st.online || last_seen.as_ref().is_none_or(|l| st.last_seen.as_deref().unwrap_or("") > l.at.as_str()))
    {
        last_seen = Some(LastSeen {
            at: st.last_seen.clone().unwrap_or_default(),
            scope: "observed_server".into(),
            provider: st.provider.clone(),
            server_id: Some(st.server_id.clone()),
            server_name: st.server_name.clone(),
            open: st.online,
            approximate: false,
        });
    }
    let sessions_status = if player.is_demo {
        DataStatus::Demo
    } else if total > 0 && links.iter().any(|l| l.provider == "battlemetrics") {
        DataStatus::Verified
    } else if total > 0 {
        DataStatus::Partial
    } else if links.iter().any(|l| l.provider == "battlemetrics") {
        DataStatus::Verified
    } else {
        DataStatus::NotConfigured
    };
    let squads = repo::squads_for_player(&ctx.pool, steam_id).await?;
    let tracked = repo::list_tracked(&ctx.pool).await?.into_iter().find(|t| t.steam_id == steam_id);
    let risk = risk_for(ctx, &player, &bans);
    let connections = connections(ctx, steam_id, &all_sessions, &squads).await?;
    Ok(PlayerProfile {
        steam2: id.steam2(),
        steam3: id.steam3(),
        account_id: id.account_id(),
        provenance: ProfileProvenance {
            profile: mk(profile_status),
            bans: mk(if player.is_demo {
                DataStatus::Demo
            } else if bans.is_empty() {
                profile_status
            } else {
                DataStatus::Verified
            }),
            hours: mk(hours_status),
            sessions: Provenance::new(if player.is_demo { "demo" } else { "tracker/battlemetrics" }, sessions_status)
                .note("Только наблюдаемые источники; отсутствие сессий ≠ отсутствие в Rust"),
            combat: Provenance::new("—", DataStatus::Unsupported)
                .note("Ни один подключённый источник не предоставляет авторитетную боевую статистику (K/D, точность)"),
        },
        aliases: repo::list_aliases(&ctx.pool, steam_id).await?,
        bans,
        links,
        sessions_total: total,
        first_session_at: first,
        last_session_at: last,
        last_seen,
        servers,
        server_stats,
        recent_sessions: all_sessions.into_iter().take(25).collect(),
        squads,
        tracked,
        risk,
        connections,
        player,
    })
}

/// Связи: тиммейты по BattleMetrics (все игроки BM, повторяющиеся совместные входы/выходы) + друзья Steam.
/// Друг Steam, который играет с целью на том же сервере, — самый сильный сигнал.
async fn connections(ctx: &Ctx, steam_id: &str, _mine: &[SessionRow], squads: &[SquadRow]) -> AppResult<Vec<Connection>> {
    let mut squad_members = std::collections::HashSet::new();
    for s in squads {
        for m in repo::list_squad_members(&ctx.pool, s.id).await? {
            squad_members.insert(m.steam_id);
        }
    }
    let friends = crate::services::linking::list_friends(ctx, steam_id).await?;
    let friend_by_id: std::collections::HashMap<&str, &crate::services::linking::FriendRow> = friends.iter().map(|f| (f.friend_id.as_str(), f)).collect();
    let mut out: Vec<Connection> = vec![];
    let mut used_friends = std::collections::HashSet::new();
    let bm_id: Option<String> =
        sqlx::query_scalar("SELECT bm_id FROM bm_players WHERE steam_id = ? ORDER BY last_seen DESC LIMIT 1").bind(steam_id).fetch_optional(&ctx.pool).await?;
    if let Some(bm) = bm_id {
        for t in crate::services::bm_service::teammates(ctx, &bm, 90).await? {
            let c = &t.candidate;
            let friend = t.steam_friend.as_ref();
            if friend.is_none() && c.confidence == "low" {
                continue;
            }
            let fr = friend.and_then(|f| friend_by_id.get(f.steam_id.as_str()).copied());
            if let Some(f) = friend {
                used_friends.insert(f.steam_id.clone());
            }
            let mut reasons = vec![];
            if friend.is_some() {
                reasons.push(if friend.is_some_and(|f| f.by == "link") {
                    "друг в Steam".to_string()
                } else {
                    "друг в Steam (совпал ник на общем сервере)".to_string()
                });
            }
            if c.joined_together > 0 {
                reasons.push(format!("заходили вместе {} раз", c.joined_together));
            }
            if c.left_together > 0 {
                reasons.push(format!("выходили вместе {} раз", c.left_together));
            }
            let sid = friend.map(|f| f.steam_id.clone()).or(t.steam_id.clone());
            out.push(Connection {
                kind: if friend.is_some() { "friend_teammate".into() } else { "teammate".into() },
                in_same_squad: sid.as_ref().is_some_and(|s| squad_members.contains(s)),
                steam_id: sid,
                bm_id: Some(c.player.clone()),
                name: t.name.clone().or(friend.and_then(|f| f.persona_name.clone())),
                steam_friend: friend.is_some(),
                friend_since: fr.and_then(|f| f.friend_since.clone()),
                in_rust_now: fr.and_then(|f| f.in_rust),
                joined_together: c.joined_together,
                left_together: c.left_together,
                co_sessions: c.co_sessions,
                overlap_secs: c.overlap_secs,
                last_together: Some(fmt_ts(c.last_together)),
                confidence: c.confidence.to_string(),
                score: c.score + if friend.is_some() { 100.0 } else { 0.0 },
                reasons,
            });
        }
    }
    // Друзья Steam без совместной игры в наблюдениях: показываем, отмечая, кто сейчас в Rust.
    for f in &friends {
        if used_friends.contains(&f.friend_id) {
            continue;
        }
        let mut reasons = vec!["друг в Steam".to_string()];
        if f.in_rust == Some(true) {
            reasons.push("сейчас в Rust".into());
        }
        out.push(Connection {
            kind: "friend".into(),
            steam_id: Some(f.friend_id.clone()),
            bm_id: None,
            name: f.persona_name.clone(),
            steam_friend: true,
            friend_since: f.friend_since.clone(),
            in_rust_now: f.in_rust,
            joined_together: 0,
            left_together: 0,
            co_sessions: 0,
            overlap_secs: 0,
            last_together: None,
            confidence: "unknown".into(),
            score: if f.in_rust == Some(true) { 1.0 } else { 0.0 },
            reasons,
            in_same_squad: squad_members.contains(&f.friend_id),
        });
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(300);
    Ok(out)
}

pub async fn ensure_known_player(ctx: &Ctx, steam_id: &str) -> AppResult<PlayerRow> {
    SteamId64::parse(steam_id)?;
    repo::get_player(&ctx.pool, steam_id).await?.ok_or_else(|| AppError::not_found("Игрок не найден в локальной базе"))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::providers::demo::demo_steam_id;

    pub async fn demo_ctx() -> Ctx {
        let pool = crate::persistence::db::open_memory().await.unwrap();
        crate::providers::demo::seed(&pool, now()).await.unwrap();
        let secrets = std::sync::Arc::new(crate::infrastructure::secrets::SecretStore::memory_only());
        let http = crate::infrastructure::http::HttpClient::new();
        let s1 = secrets.clone();
        Ctx {
            pool,
            mode: crate::config::Mode::Demo,
            settings: crate::config::AppSettings::default(),
            steam: std::sync::Arc::new(crate::providers::steam::SteamProvider::new(
                http.clone(),
                std::sync::Arc::new(move || s1.get(crate::infrastructure::secrets::SecretKey::SteamApiKey)),
            )),
            bm: std::sync::Arc::new(crate::providers::battlemetrics::BattleMetricsProvider::new(http.clone(), std::sync::Arc::new(|| None))),
            bmru: std::sync::Arc::new(crate::providers::bmru::BmRuProvider::new(http.clone(), std::sync::Arc::new(|| None))),
            secrets,
            http,
        }
    }

    #[tokio::test]
    async fn demo_search_and_profile() {
        let ctx = demo_ctx().await;
        let id = demo_steam_id(1);
        let r = search(&ctx, id.as_str()).await.unwrap();
        assert_eq!(r.resolved_steam_id.as_deref(), Some(id.as_str()));
        let p = get_profile(&ctx, id.as_str()).await.unwrap();
        assert_eq!(p.provenance.profile.status, DataStatus::Demo);
        assert_eq!(p.provenance.combat.status, DataStatus::Unsupported);
        assert!(p.sessions_total > 0);
        assert!(p.connections.iter().any(|c| c.kind == "friend_teammate" && c.name.as_deref() == Some("DEMO·Клещ")), "друг + играют вместе");
        assert!(p.connections.iter().any(|c| c.kind == "friend" && c.name.as_deref() == Some("DEMO·Новичок")), "друг без совместной игры");
        assert!(p.connections.iter().filter(|c| c.kind == "teammate").all(|c| c.confidence != "low"));
        // реальный SteamID в демо-режиме не ищется
        assert!(search(&ctx, "76561197960287930").await.is_err());
        // ник — кандидаты из локальной базы, без угадывания
        let r = search(&ctx, "Ворон").await.unwrap();
        assert!(r.resolved_steam_id.is_none());
        assert_eq!(r.candidates.len(), 1);
    }

    #[tokio::test]
    async fn live_without_key_is_not_configured_not_fake() {
        let mut ctx = demo_ctx().await;
        ctx.mode = crate::config::Mode::Live;
        ctx.pool = crate::persistence::db::open_memory().await.unwrap();
        let r = search(&ctx, "76561197960287930").await.unwrap();
        assert!(r.notices.iter().any(|n| n.level == "warning"));
        let p = get_profile(&ctx, "76561197960287930").await.unwrap();
        assert_eq!(p.provenance.profile.status, DataStatus::NotConfigured);
        assert!(p.player.persona_name.is_none());
    }
}
