//! Поиск, профиль, обновление игрока из источников.

use super::to_bounds;
use crate::domain::analytics::co_sessions;
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
    // BattleMetrics: уже привязан — обновляем; нет — пробуем точное совпадение по steamID.
    if ctx.bm.is_configured() {
        let existing = repo::list_links(&ctx.pool, sid).await?.into_iter().find(|l| l.provider == "battlemetrics");
        match existing {
            Some(l) => sync_bm(ctx, id, &BattleMetricsPlayerId(l.external_id), notices).await?,
            None => match ctx.bm.match_steam_id(id).await {
                Ok(Some(bm)) => {
                    save_bm_link(ctx, sid, &bm, "exact_identifier").await?;
                    notices.push(Notice { level: "info", text: format!("BattleMetrics: найден игрок {} по SteamID", bm.0) });
                    sync_bm(ctx, id, &bm, notices).await?;
                }
                Ok(None) => notices.push(Notice {
                    level: "warning",
                    text: "BattleMetrics не раскрывает SteamID обычным токенам, поэтому игрок не найден автоматически. Нажмите «Привязать BattleMetrics» в карточке и выберите игрока по нику или вставьте ссылку на его страницу BattleMetrics.".into(),
                }),
                Err(e) => {
                    record_err(ctx, "battlemetrics", &e).await;
                    notices.push(Notice { level: "warning", text: format!("BattleMetrics: {e}. Можно привязать игрока вручную в карточке.") });
                }
            },
        }
    }
    Ok(())
}

async fn save_bm_link(ctx: &Ctx, sid: &str, bm: &BattleMetricsPlayerId, evidence: &str) -> AppResult<()> {
    repo::upsert_link(
        &ctx.pool,
        &ProviderLinkRow {
            steam_id: sid.into(),
            provider: "battlemetrics".into(),
            external_id: bm.0.clone(),
            match_evidence: evidence.into(),
            confidence: if evidence == "exact_identifier" { "high".into() } else { "medium".into() },
            source_url: Some(format!("https://www.battlemetrics.com/players/{}", bm.0)),
            linked_at: fmt_ts(now()),
        },
    )
    .await?;
    Ok(())
}

/// Загружает из BattleMetrics серверы игрока (онлайн, время, первый/последний раз), ники и сессии.
pub async fn sync_bm(ctx: &Ctx, id: &SteamId64, bm: &BattleMetricsPlayerId, notices: &mut Vec<Notice>) -> AppResult<()> {
    let sid = id.as_str();
    let t = now();
    match ctx.bm.player_info(bm).await {
        Ok(info) => {
            repo::record_health_ok(&ctx.pool, "battlemetrics").await?;
            for n in info.names.iter().chain(info.name.iter()) {
                repo::record_alias(&ctx.pool, sid, n, "battlemetrics", t).await?;
            }
            for st in &info.servers {
                let existing = repo::get_server(&ctx.pool, &st.server_id).await?;
                repo::upsert_server(
                    &ctx.pool,
                    &ServerRow {
                        id: st.server_id.clone(),
                        provider: "battlemetrics".into(),
                        external_id: st.server_id.trim_start_matches("battlemetrics:").into(),
                        name: st.server_name.clone(),
                        address: st.address.clone(),
                        country: st.country.clone(),
                        map: existing.as_ref().and_then(|e| e.map.clone()),
                        players: st.players,
                        max_players: st.max_players,
                        queue: None,
                        online: Some(true),
                        trust: None,
                        last_wipe_at: None,
                        next_wipe_at: None,
                        followed: existing.as_ref().is_some_and(|e| e.followed),
                        source_url: Some(format!("https://www.battlemetrics.com/servers/rust/{}", st.server_id.trim_start_matches("battlemetrics:"))),
                        fetched_at: fmt_ts(t),
                        is_demo: false,
                    },
                )
                .await?;
                repo::upsert_server_stat(&ctx.pool, sid, st, "battlemetrics", t).await?;
            }
            notices.push(Notice { level: "info", text: format!("BattleMetrics: серверов игрока: {}", info.servers.len()) });
        }
        Err(e) => {
            record_err(ctx, "battlemetrics", &e).await;
            notices.push(Notice { level: "error", text: format!("BattleMetrics, профиль игрока: {e}") });
            return Ok(());
        }
    }
    import_bm_sessions(ctx, id, bm, notices).await
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
    save_bm_link(ctx, steam_id, &bm, evidence).await?;
    let mut notices =
        vec![Notice { level: "info", text: format!("Привязан игрок BattleMetrics «{}» ({})", info.name.clone().unwrap_or_default(), bm.0) }];
    sync_bm(ctx, &id, &bm, &mut notices).await?;
    Ok(notices)
}

pub async fn unlink_bm(ctx: &Ctx, steam_id: &str) -> AppResult<()> {
    SteamId64::parse(steam_id)?;
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

async fn import_bm_sessions(ctx: &Ctx, id: &SteamId64, bm: &crate::domain::ids::BattleMetricsPlayerId, notices: &mut Vec<Notice>) -> AppResult<()> {
    let sessions = match ctx.bm.sessions(bm).await {
        Ok(s) => s,
        Err(e) => {
            notices.push(Notice { level: "warning", text: format!("BattleMetrics сессии: {e}") });
            return Ok(());
        }
    };
    let t = now();
    let mut oldest = t;
    for s in &sessions {
        if let (Some(server_id), Some(name)) = (&s.server_id, &s.server_name)
            && repo::get_server(&ctx.pool, server_id).await?.is_none()
        {
            repo::upsert_server(
                &ctx.pool,
                &ServerRow {
                    id: server_id.clone(),
                    provider: "battlemetrics".into(),
                    external_id: server_id.trim_start_matches("battlemetrics:").into(),
                    name: name.clone(),
                    address: None,
                    country: None,
                    map: None,
                    players: None,
                    max_players: None,
                    queue: None,
                    online: None,
                    trust: None,
                    last_wipe_at: None,
                    next_wipe_at: None,
                    followed: false,
                    source_url: Some(format!("https://www.battlemetrics.com/servers/rust/{}", server_id.trim_start_matches("battlemetrics:"))),
                    fetched_at: fmt_ts(t),
                    is_demo: false,
                },
            )
            .await?;
        }
        oldest = oldest.min(s.start);
        repo::upsert_session(
            &ctx.pool,
            &repo::NewSession {
                steam_id: id.as_str(),
                provider: "battlemetrics",
                kind: "provider_reported",
                server_id: s.server_id.as_deref(),
                start_lower: s.start,
                start_upper: s.start,
                end_lower: s.stop.unwrap_or(t),
                end_upper: s.stop,
                is_open: s.stop.is_none(),
                external_id: Some(&s.external_id),
            },
        )
        .await?;
        if let Some(n) = &s.player_name {
            repo::record_alias(&ctx.pool, id.as_str(), n, "battlemetrics", s.start).await?;
        }
    }
    // Покрытие: BattleMetrics видит игрока только на отслеживаемых им серверах; окно = загруженная история.
    if !sessions.is_empty() {
        repo::add_coverage(&ctx.pool, id.as_str(), "battlemetrics", oldest, t).await?;
    }
    notices.push(Notice { level: "info", text: format!("BattleMetrics: импортировано сессий: {}", sessions.len()) });
    Ok(())
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
    pub steam_id: String,
    pub persona_name: Option<String>,
    pub co_sessions: usize,
    pub co_online_secs: i64,
    pub servers: Vec<String>,
    pub last_together: String,
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

/// Связи — только по реальным совместным сессиям на одном сервере (≥ 20 мин, ≥ 2 раза).
async fn connections(ctx: &Ctx, steam_id: &str, mine: &[SessionRow], squads: &[SquadRow]) -> AppResult<Vec<Connection>> {
    let mine_b = to_bounds(mine);
    let t = now();
    let from = fmt_ts(t - Duration::days(90));
    let mut squad_members = std::collections::HashSet::new();
    for s in squads {
        for m in repo::list_squad_members(&ctx.pool, s.id).await? {
            squad_members.insert(m.steam_id);
        }
    }
    let mut out = Vec::new();
    for other in repo::list_players(&ctx.pool, None, false, 500, 0).await? {
        if other.steam_id == steam_id {
            continue;
        }
        let theirs = repo::list_sessions(&ctx.pool, &other.steam_id, &from, &fmt_ts(t)).await?;
        if theirs.is_empty() {
            continue;
        }
        let ev = co_sessions(&mine_b, &to_bounds(&theirs), 20 * 60);
        if ev.len() < 2 {
            continue;
        }
        let mut servers: Vec<String> = ev.iter().map(|e| e.server_id.clone()).collect();
        servers.sort();
        servers.dedup();
        out.push(Connection {
            in_same_squad: squad_members.contains(&other.steam_id),
            steam_id: other.steam_id,
            persona_name: other.persona_name,
            co_sessions: ev.len(),
            co_online_secs: ev.iter().map(|e| e.overlap_secs).sum(),
            servers,
            last_together: fmt_ts(ev[0].end),
        });
    }
    out.sort_by_key(|x| std::cmp::Reverse(x.co_sessions));
    out.truncate(20);
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
        assert!(!p.connections.is_empty(), "игроки одной группы имеют совместные сессии");
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
