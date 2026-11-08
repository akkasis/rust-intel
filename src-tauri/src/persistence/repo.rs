//! Репозитории. Только параметризованные запросы (`&'static str` + bind), идемпотентные вставки.

use super::models::*;
use crate::domain::bans::BanRecord;
use crate::infrastructure::clock::{fmt_ts, now};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

type R<T> = Result<T, sqlx::Error>;

// ----------------------------------------------------------------- settings

pub async fn get_setting(pool: &SqlitePool, key: &str) -> R<Option<String>> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(pool).await
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> R<()> {
    sqlx::query(
        "INSERT INTO settings(key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(fmt_ts(now()))
    .execute(pool)
    .await?;
    Ok(())
}

// ------------------------------------------------------------------ players

#[derive(Debug, Clone, Default)]
pub struct PlayerUpsert {
    pub steam_id: String,
    pub persona_name: Option<String>,
    pub avatar_url: Option<String>,
    pub profile_url: Option<String>,
    pub visibility: Option<String>,
    pub account_created_at: Option<String>,
    pub country_code: Option<String>,
    pub rust_hours: Option<f64>,
    pub rust_hours_status: Option<String>,
    pub steam_in_game: Option<bool>,
    pub steam_status: Option<String>,
    pub steam_fetched_at: Option<String>,
    pub is_demo: bool,
}

/// Создаёт игрока-заглушку (только SteamID), если его ещё нет.
pub async fn ensure_player(pool: &SqlitePool, steam_id: &str, is_demo: bool) -> R<()> {
    let t = fmt_ts(now());
    sqlx::query("INSERT OR IGNORE INTO players(steam_id, is_demo, created_at, updated_at) VALUES (?, ?, ?, ?)")
        .bind(steam_id)
        .bind(is_demo)
        .bind(&t)
        .bind(&t)
        .execute(pool)
        .await?;
    Ok(())
}

/// Обновляет только переданные поля (None = не трогать).
pub async fn upsert_player(pool: &SqlitePool, p: &PlayerUpsert) -> R<()> {
    ensure_player(pool, &p.steam_id, p.is_demo).await?;
    sqlx::query(
        "UPDATE players SET
            persona_name = COALESCE(?, persona_name),
            avatar_url = COALESCE(?, avatar_url),
            profile_url = COALESCE(?, profile_url),
            visibility = COALESCE(?, visibility),
            account_created_at = COALESCE(?, account_created_at),
            country_code = COALESCE(?, country_code),
            rust_hours = CASE WHEN ? IS NOT NULL THEN ? ELSE rust_hours END,
            rust_hours_status = COALESCE(?, rust_hours_status),
            steam_in_game = COALESCE(?, steam_in_game),
            steam_status = COALESCE(?, steam_status),
            steam_fetched_at = COALESCE(?, steam_fetched_at),
            updated_at = ?
         WHERE steam_id = ?",
    )
    .bind(&p.persona_name)
    .bind(&p.avatar_url)
    .bind(&p.profile_url)
    .bind(&p.visibility)
    .bind(&p.account_created_at)
    .bind(&p.country_code)
    .bind(p.rust_hours_status.as_ref())
    .bind(p.rust_hours)
    .bind(&p.rust_hours_status)
    .bind(p.steam_in_game)
    .bind(&p.steam_status)
    .bind(&p.steam_fetched_at)
    .bind(fmt_ts(now()))
    .bind(&p.steam_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_player(pool: &SqlitePool, steam_id: &str) -> R<Option<PlayerRow>> {
    sqlx::query_as("SELECT * FROM players WHERE steam_id = ?").bind(steam_id).fetch_optional(pool).await
}

pub async fn list_players(pool: &SqlitePool, query: Option<&str>, favorites_only: bool, limit: i64, offset: i64) -> R<Vec<PlayerRow>> {
    let like = query.filter(|q| !q.trim().is_empty()).map(|q| format!("%{}%", q.trim()));
    sqlx::query_as(
        "SELECT * FROM players
         WHERE (? IS NULL OR persona_name LIKE ? OR steam_id LIKE ?)
           AND (? = 0 OR favorite = 1)
         ORDER BY favorite DESC, updated_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .bind(favorites_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn count_players(pool: &SqlitePool) -> R<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM players").fetch_one(pool).await
}

pub async fn set_favorite(pool: &SqlitePool, steam_id: &str, fav: bool) -> R<()> {
    sqlx::query("UPDATE players SET favorite = ? WHERE steam_id = ?").bind(fav).bind(steam_id).execute(pool).await?;
    Ok(())
}

pub async fn set_notes(pool: &SqlitePool, steam_id: &str, notes: Option<&str>) -> R<()> {
    sqlx::query("UPDATE players SET notes = ? WHERE steam_id = ?").bind(notes).bind(steam_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_player(pool: &SqlitePool, steam_id: &str) -> R<()> {
    sqlx::query("DELETE FROM players WHERE steam_id = ?").bind(steam_id).execute(pool).await?;
    Ok(())
}

pub async fn purge_player_history(pool: &SqlitePool, steam_id: &str) -> R<()> {
    let mut tx = pool.begin().await?;
    for q in [
        "DELETE FROM observations WHERE steam_id = ?",
        "DELETE FROM sessions WHERE steam_id = ?",
        "DELETE FROM coverage_windows WHERE steam_id = ?",
        "DELETE FROM events WHERE steam_id = ?",
    ] {
        sqlx::query(q).bind(steam_id).execute(&mut *tx).await?;
    }
    tx.commit().await
}

pub async fn record_alias(pool: &SqlitePool, steam_id: &str, name: &str, source: &str, at: DateTime<Utc>) -> R<bool> {
    let t = fmt_ts(at);
    let r = sqlx::query(
        "INSERT INTO aliases(steam_id, name, source, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(steam_id, name, source) DO UPDATE SET last_seen_at = MAX(last_seen_at, excluded.last_seen_at)",
    )
    .bind(steam_id)
    .bind(name)
    .bind(source)
    .bind(&t)
    .bind(&t)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn list_aliases(pool: &SqlitePool, steam_id: &str) -> R<Vec<AliasRow>> {
    sqlx::query_as("SELECT name, source, first_seen_at, last_seen_at FROM aliases WHERE steam_id = ? ORDER BY last_seen_at DESC")
        .bind(steam_id)
        .fetch_all(pool)
        .await
}

pub async fn add_search_history(pool: &SqlitePool, query: &str, steam_id: Option<&str>) -> R<()> {
    sqlx::query("INSERT INTO search_history(query, steam_id, searched_at) VALUES (?, ?, ?)")
        .bind(query)
        .bind(steam_id)
        .bind(fmt_ts(now()))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM search_history WHERE id NOT IN (SELECT id FROM search_history ORDER BY id DESC LIMIT 50)").execute(pool).await?;
    Ok(())
}

pub async fn recent_searches(pool: &SqlitePool) -> R<Vec<SearchHistoryRow>> {
    sqlx::query_as("SELECT query, steam_id, MAX(searched_at) AS searched_at FROM search_history GROUP BY query ORDER BY searched_at DESC LIMIT 10")
        .fetch_all(pool)
        .await
}

// ----------------------------------------------------------- provider links

pub async fn upsert_link(pool: &SqlitePool, l: &ProviderLinkRow) -> R<()> {
    sqlx::query(
        "INSERT INTO provider_links(steam_id, provider, external_id, match_evidence, confidence, source_url, linked_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(steam_id, provider) DO UPDATE SET external_id = excluded.external_id,
            match_evidence = excluded.match_evidence, confidence = excluded.confidence,
            source_url = excluded.source_url, linked_at = excluded.linked_at",
    )
    .bind(&l.steam_id)
    .bind(&l.provider)
    .bind(&l.external_id)
    .bind(&l.match_evidence)
    .bind(&l.confidence)
    .bind(&l.source_url)
    .bind(&l.linked_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_links(pool: &SqlitePool, steam_id: &str) -> R<Vec<ProviderLinkRow>> {
    sqlx::query_as("SELECT steam_id, provider, external_id, match_evidence, confidence, source_url, linked_at FROM provider_links WHERE steam_id = ?")
        .bind(steam_id)
        .fetch_all(pool)
        .await
}

// --------------------------------------------------------------------- bans

/// Атомарно заменяет баны данного провайдера для игрока.
pub async fn replace_bans(pool: &SqlitePool, steam_id: &str, provider: &str, bans: &[BanRecord]) -> R<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM bans WHERE steam_id = ? AND provider = ?").bind(steam_id).bind(provider).execute(&mut *tx).await?;
    for b in bans {
        sqlx::query(
            "INSERT OR REPLACE INTO bans(steam_id, provider, kind, project_name, count, active, days_since_last, reason, revoked, source_url, observed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(steam_id)
        .bind(provider)
        .bind(b.kind.as_str())
        .bind(b.project_name.clone().unwrap_or_default())
        .bind(b.count)
        .bind(b.active)
        .bind(b.days_since_last)
        .bind(&b.reason)
        .bind(b.revoked)
        .bind(&b.source_url)
        .bind(fmt_ts(b.observed_at))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

pub async fn list_bans(pool: &SqlitePool, steam_id: &str) -> R<Vec<BanRow>> {
    sqlx::query_as("SELECT steam_id, provider, kind, project_name, count, active, days_since_last, reason, revoked, source_url, observed_at FROM bans WHERE steam_id = ? ORDER BY kind")
        .bind(steam_id)
        .fetch_all(pool)
        .await
}

pub async fn list_all_bans(pool: &SqlitePool) -> R<Vec<BanRow>> {
    sqlx::query_as("SELECT steam_id, provider, kind, project_name, count, active, days_since_last, reason, revoked, source_url, observed_at FROM bans")
        .fetch_all(pool)
        .await
}

// ------------------------------------------------------------------ servers

pub async fn upsert_server(pool: &SqlitePool, s: &ServerRow) -> R<()> {
    sqlx::query(
        "INSERT INTO servers(id, provider, external_id, name, address, country, map, players, max_players, queue, online, trust,
                             last_wipe_at, next_wipe_at, followed, source_url, fetched_at, is_demo)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, address = excluded.address, country = excluded.country,
            map = excluded.map, players = excluded.players, max_players = excluded.max_players, queue = excluded.queue,
            online = excluded.online, trust = excluded.trust,
            last_wipe_at = COALESCE(excluded.last_wipe_at, servers.last_wipe_at),
            next_wipe_at = COALESCE(excluded.next_wipe_at, servers.next_wipe_at),
            source_url = excluded.source_url, fetched_at = excluded.fetched_at",
    )
    .bind(&s.id)
    .bind(&s.provider)
    .bind(&s.external_id)
    .bind(&s.name)
    .bind(&s.address)
    .bind(&s.country)
    .bind(&s.map)
    .bind(s.players)
    .bind(s.max_players)
    .bind(s.queue)
    .bind(s.online)
    .bind(&s.trust)
    .bind(&s.last_wipe_at)
    .bind(&s.next_wipe_at)
    .bind(s.followed)
    .bind(&s.source_url)
    .bind(&s.fetched_at)
    .bind(s.is_demo)
    .execute(pool)
    .await?;
    if let Some(w) = &s.last_wipe_at {
        crate::services::insights_service::record_wipe(pool, &s.id, w, &s.provider).await?;
    }
    Ok(())
}

pub async fn list_servers(pool: &SqlitePool, query: Option<&str>, followed_only: bool, limit: i64) -> R<Vec<ServerRow>> {
    let like = query.filter(|q| !q.trim().is_empty()).map(|q| format!("%{}%", q.trim()));
    sqlx::query_as(
        "SELECT * FROM servers WHERE (? IS NULL OR name LIKE ? OR address LIKE ?) AND (? = 0 OR followed = 1)
         ORDER BY followed DESC, COALESCE(players, -1) DESC LIMIT ?",
    )
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .bind(followed_only)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn get_server(pool: &SqlitePool, id: &str) -> R<Option<ServerRow>> {
    sqlx::query_as("SELECT * FROM servers WHERE id = ?").bind(id).fetch_optional(pool).await
}

pub async fn set_server_followed(pool: &SqlitePool, id: &str, followed: bool) -> R<()> {
    sqlx::query("UPDATE servers SET followed = ? WHERE id = ?").bind(followed).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn insert_population(pool: &SqlitePool, server_id: &str, points: &[(String, f64)], source: &str) -> R<()> {
    let mut tx = pool.begin().await?;
    for (t, p) in points {
        sqlx::query("INSERT OR REPLACE INTO server_population(server_id, t, players, source) VALUES (?, ?, ?, ?)")
            .bind(server_id)
            .bind(t)
            .bind(p)
            .bind(source)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

pub async fn get_population(pool: &SqlitePool, server_id: &str, since: &str) -> R<Vec<PopulationPoint>> {
    sqlx::query_as("SELECT t, players, source FROM server_population WHERE server_id = ? AND t >= ? ORDER BY t")
        .bind(server_id)
        .bind(since)
        .fetch_all(pool)
        .await
}

// ------------------------------------------------ observations / sessions

pub async fn insert_observation(
    pool: &SqlitePool,
    steam_id: &str,
    provider: &str,
    at: DateTime<Utc>,
    state: &str,
    server_id: Option<&str>,
    poll_interval_secs: i64,
) -> R<bool> {
    let r = sqlx::query("INSERT OR IGNORE INTO observations(steam_id, provider, observed_at, state, server_id, poll_interval_secs) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(steam_id)
        .bind(provider)
        .bind(fmt_ts(at))
        .bind(state)
        .bind(server_id)
        .bind(poll_interval_secs)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn list_observations(pool: &SqlitePool, steam_id: &str, since: &str, limit: i64) -> R<Vec<ObservationRow>> {
    sqlx::query_as(
        "SELECT provider, observed_at, state, server_id, poll_interval_secs FROM observations
         WHERE steam_id = ? AND observed_at >= ? ORDER BY observed_at DESC LIMIT ?",
    )
    .bind(steam_id)
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub struct NewSession<'a> {
    pub steam_id: &'a str,
    pub provider: &'a str,
    pub kind: &'a str,
    pub server_id: Option<&'a str>,
    pub start_lower: DateTime<Utc>,
    pub start_upper: DateTime<Utc>,
    pub end_lower: DateTime<Utc>,
    pub end_upper: Option<DateTime<Utc>>,
    pub is_open: bool,
    pub external_id: Option<&'a str>,
}

/// Идемпотентно: по (steam_id, provider, start_upper) обновляет конец сессии.
pub async fn upsert_session(pool: &SqlitePool, s: &NewSession<'_>) -> R<()> {
    sqlx::query(
        "INSERT INTO sessions(steam_id, provider, kind, server_id, start_lower, start_upper, end_lower, end_upper, is_open, external_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(steam_id, provider, start_upper) DO UPDATE SET
            end_lower = excluded.end_lower, end_upper = excluded.end_upper, is_open = excluded.is_open,
            server_id = COALESCE(excluded.server_id, sessions.server_id)",
    )
    .bind(s.steam_id)
    .bind(s.provider)
    .bind(s.kind)
    .bind(s.server_id)
    .bind(fmt_ts(s.start_lower))
    .bind(fmt_ts(s.start_upper))
    .bind(fmt_ts(s.end_lower))
    .bind(s.end_upper.map(fmt_ts))
    .bind(s.is_open)
    .bind(s.external_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_sessions(pool: &SqlitePool, steam_id: &str, from: &str, to: &str) -> R<Vec<SessionRow>> {
    sqlx::query_as(
        "SELECT s.id, s.steam_id, s.provider, s.kind, s.server_id, sv.name AS server_name, s.start_lower, s.start_upper,
                s.end_lower, s.end_upper, s.is_open, s.external_id
         FROM sessions s LEFT JOIN servers sv ON sv.id = s.server_id
         WHERE s.steam_id = ? AND s.end_lower >= ? AND s.start_lower <= ?
         ORDER BY s.start_upper DESC",
    )
    .bind(steam_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await
}

pub async fn open_session(pool: &SqlitePool, steam_id: &str, provider: &str) -> R<Option<SessionRow>> {
    sqlx::query_as(
        "SELECT s.id, s.steam_id, s.provider, s.kind, s.server_id, NULL AS server_name, s.start_lower, s.start_upper,
                s.end_lower, s.end_upper, s.is_open, s.external_id
         FROM sessions s WHERE s.steam_id = ? AND s.provider = ? AND s.is_open = 1 ORDER BY s.start_upper DESC LIMIT 1",
    )
    .bind(steam_id)
    .bind(provider)
    .fetch_optional(pool)
    .await
}

pub async fn session_span(pool: &SqlitePool, steam_id: &str) -> R<(Option<String>, Option<String>, i64)> {
    let r: (Option<String>, Option<String>, i64) =
        sqlx::query_as("SELECT MIN(start_upper), MAX(end_lower), COUNT(*) FROM sessions WHERE steam_id = ?").bind(steam_id).fetch_one(pool).await?;
    Ok(r)
}

pub async fn add_coverage(pool: &SqlitePool, steam_id: &str, provider: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> R<()> {
    // Продлеваем окно, если оно заканчивается ровно в start, иначе создаём новое.
    let s = fmt_ts(start);
    let e = fmt_ts(end);
    let r = sqlx::query("UPDATE coverage_windows SET end_at = ? WHERE steam_id = ? AND provider = ? AND end_at = ?")
        .bind(&e)
        .bind(steam_id)
        .bind(provider)
        .bind(&s)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        sqlx::query(
            "INSERT INTO coverage_windows(steam_id, provider, start_at, end_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(steam_id, provider, start_at) DO UPDATE SET end_at = MAX(end_at, excluded.end_at)",
        )
        .bind(steam_id)
        .bind(provider)
        .bind(&s)
        .bind(&e)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn list_coverage(pool: &SqlitePool, steam_id: &str, from: &str, to: &str) -> R<Vec<CoverageRow>> {
    sqlx::query_as("SELECT provider, start_at, end_at FROM coverage_windows WHERE steam_id = ? AND end_at >= ? AND start_at <= ? ORDER BY start_at")
        .bind(steam_id)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await
}

// ------------------------------------------------------------------ tracker

pub async fn set_tracked(pool: &SqlitePool, steam_id: &str, enabled: bool) -> R<()> {
    sqlx::query(
        "INSERT INTO tracked_players(steam_id, enabled, added_at) VALUES (?, ?, ?)
         ON CONFLICT(steam_id) DO UPDATE SET enabled = excluded.enabled",
    )
    .bind(steam_id)
    .bind(enabled)
    .bind(fmt_ts(now()))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_tracked(pool: &SqlitePool, steam_id: &str) -> R<()> {
    sqlx::query("DELETE FROM tracked_players WHERE steam_id = ?").bind(steam_id).execute(pool).await?;
    Ok(())
}

pub async fn set_tracked_notify(pool: &SqlitePool, steam_id: &str, notify: bool) -> R<()> {
    sqlx::query("UPDATE tracked_players SET notify = ? WHERE steam_id = ?").bind(notify).bind(steam_id).execute(pool).await?;
    Ok(())
}

pub async fn list_tracked(pool: &SqlitePool) -> R<Vec<TrackedRow>> {
    sqlx::query_as(
        "SELECT t.steam_id, t.enabled, t.notify, t.added_at, t.last_state, t.last_server, t.last_polled_at, t.last_success_at,
                p.persona_name, p.avatar_url, p.is_demo
         FROM tracked_players t JOIN players p ON p.steam_id = t.steam_id ORDER BY p.persona_name",
    )
    .fetch_all(pool)
    .await
}

pub async fn update_tracked_state(pool: &SqlitePool, steam_id: &str, state: &str, server: Option<&str>, polled_at: DateTime<Utc>, success: bool) -> R<()> {
    let t = fmt_ts(polled_at);
    sqlx::query(
        "UPDATE tracked_players SET last_state = ?, last_server = ?, last_polled_at = ?,
            last_success_at = CASE WHEN ? THEN ? ELSE last_success_at END
         WHERE steam_id = ?",
    )
    .bind(state)
    .bind(server)
    .bind(&t)
    .bind(success)
    .bind(&t)
    .bind(steam_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ------------------------------------------------------------------- events

pub struct NewEvent<'a> {
    pub steam_id: Option<&'a str>,
    pub provider: &'a str,
    pub kind: &'a str,
    pub dedup_key: String,
    pub occurred_lower: DateTime<Utc>,
    pub occurred_upper: DateTime<Utc>,
    pub approximate: bool,
    pub server_id: Option<&'a str>,
    pub details: serde_json::Value,
}

/// true — событие новое (дубликаты по dedup_key игнорируются).
pub async fn insert_event(pool: &SqlitePool, e: &NewEvent<'_>) -> R<Option<i64>> {
    let r = sqlx::query(
        "INSERT OR IGNORE INTO events(steam_id, provider, kind, dedup_key, occurred_lower, occurred_upper, approximate, server_id, details, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.steam_id)
    .bind(e.provider)
    .bind(e.kind)
    .bind(&e.dedup_key)
    .bind(fmt_ts(e.occurred_lower))
    .bind(fmt_ts(e.occurred_upper))
    .bind(e.approximate)
    .bind(e.server_id)
    .bind(e.details.to_string())
    .bind(fmt_ts(now()))
    .execute(pool)
    .await?;
    Ok((r.rows_affected() > 0).then(|| r.last_insert_rowid()))
}

pub async fn list_events(pool: &SqlitePool, steam_id: Option<&str>, kind: Option<&str>, unread_only: bool, limit: i64, offset: i64) -> R<Vec<EventRow>> {
    sqlx::query_as(
        "SELECT e.id, e.steam_id, p.persona_name, e.provider, e.kind, e.occurred_lower, e.occurred_upper, e.approximate,
                e.server_id, s.name AS server_name, e.details, e.created_at, e.is_read, e.notified
         FROM events e LEFT JOIN players p ON p.steam_id = e.steam_id LEFT JOIN servers s ON s.id = e.server_id
         WHERE (? IS NULL OR e.steam_id = ?) AND (? IS NULL OR e.kind = ?) AND (? = 0 OR e.is_read = 0)
         ORDER BY e.occurred_upper DESC, e.id DESC LIMIT ? OFFSET ?",
    )
    .bind(steam_id)
    .bind(steam_id)
    .bind(kind)
    .bind(kind)
    .bind(unread_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn count_unread_events(pool: &SqlitePool) -> R<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE is_read = 0").fetch_one(pool).await
}

pub async fn mark_events_read(pool: &SqlitePool, ids: Option<&[i64]>) -> R<()> {
    match ids {
        None => {
            sqlx::query("UPDATE events SET is_read = 1").execute(pool).await?;
        }
        Some(ids) => {
            let mut tx = pool.begin().await?;
            for id in ids {
                sqlx::query("UPDATE events SET is_read = 1 WHERE id = ?").bind(id).execute(&mut *tx).await?;
            }
            tx.commit().await?;
        }
    }
    Ok(())
}

pub async fn mark_event_notified(pool: &SqlitePool, id: i64) -> R<()> {
    sqlx::query("UPDATE events SET notified = 1 WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

// ------------------------------------------------------------------- squads

pub async fn list_squads(pool: &SqlitePool) -> R<Vec<SquadRow>> {
    sqlx::query_as(
        "SELECT s.id, s.name, s.color, s.notes, s.created_at, s.updated_at, COUNT(m.steam_id) AS member_count
         FROM squads s LEFT JOIN squad_members m ON m.squad_id = s.id GROUP BY s.id ORDER BY s.name",
    )
    .fetch_all(pool)
    .await
}

pub async fn get_squad(pool: &SqlitePool, id: i64) -> R<Option<SquadRow>> {
    sqlx::query_as(
        "SELECT s.id, s.name, s.color, s.notes, s.created_at, s.updated_at, COUNT(m.steam_id) AS member_count
         FROM squads s LEFT JOIN squad_members m ON m.squad_id = s.id WHERE s.id = ? GROUP BY s.id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn create_squad(pool: &SqlitePool, name: &str, color: &str, notes: Option<&str>) -> R<i64> {
    let t = fmt_ts(now());
    let r = sqlx::query("INSERT INTO squads(name, color, notes, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind(name)
        .bind(color)
        .bind(notes)
        .bind(&t)
        .bind(&t)
        .execute(pool)
        .await?;
    Ok(r.last_insert_rowid())
}

pub async fn update_squad(pool: &SqlitePool, id: i64, name: &str, color: &str, notes: Option<&str>) -> R<()> {
    sqlx::query("UPDATE squads SET name = ?, color = ?, notes = ?, updated_at = ? WHERE id = ?")
        .bind(name)
        .bind(color)
        .bind(notes)
        .bind(fmt_ts(now()))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_squad(pool: &SqlitePool, id: i64) -> R<()> {
    sqlx::query("DELETE FROM squads WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn list_squad_members(pool: &SqlitePool, id: i64) -> R<Vec<SquadMemberRow>> {
    sqlx::query_as(
        "SELECT m.steam_id, p.persona_name, p.avatar_url, m.provenance, m.added_at, p.is_demo
         FROM squad_members m JOIN players p ON p.steam_id = m.steam_id WHERE m.squad_id = ? ORDER BY m.added_at",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

pub async fn add_squad_member(pool: &SqlitePool, id: i64, steam_id: &str, provenance: &str) -> R<()> {
    sqlx::query("INSERT OR IGNORE INTO squad_members(squad_id, steam_id, provenance, added_at) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(steam_id)
        .bind(provenance)
        .bind(fmt_ts(now()))
        .execute(pool)
        .await?;
    sqlx::query("UPDATE squads SET updated_at = ? WHERE id = ?").bind(fmt_ts(now())).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn remove_squad_member(pool: &SqlitePool, id: i64, steam_id: &str) -> R<()> {
    sqlx::query("DELETE FROM squad_members WHERE squad_id = ? AND steam_id = ?").bind(id).bind(steam_id).execute(pool).await?;
    Ok(())
}

pub async fn squads_for_player(pool: &SqlitePool, steam_id: &str) -> R<Vec<SquadRow>> {
    sqlx::query_as(
        "SELECT s.id, s.name, s.color, s.notes, s.created_at, s.updated_at,
                (SELECT COUNT(*) FROM squad_members mm WHERE mm.squad_id = s.id) AS member_count
         FROM squads s JOIN squad_members m ON m.squad_id = s.id WHERE m.steam_id = ?",
    )
    .bind(steam_id)
    .fetch_all(pool)
    .await
}

// ------------------------------------------------------------------- health

pub async fn record_health_ok(pool: &SqlitePool, provider: &str) -> R<()> {
    let t = fmt_ts(now());
    sqlx::query(
        "INSERT INTO provider_health(provider, status, last_success_at, consecutive_failures, checked_at) VALUES (?, 'ok', ?, 0, ?)
         ON CONFLICT(provider) DO UPDATE SET status = 'ok', last_success_at = excluded.last_success_at,
            consecutive_failures = 0, checked_at = excluded.checked_at",
    )
    .bind(provider)
    .bind(&t)
    .bind(&t)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_health_error(pool: &SqlitePool, provider: &str, status: &str, kind: &str, msg: &str) -> R<()> {
    let t = fmt_ts(now());
    sqlx::query(
        "INSERT INTO provider_health(provider, status, last_error_at, last_error_kind, last_error, consecutive_failures, checked_at)
         VALUES (?, ?, ?, ?, ?, 1, ?)
         ON CONFLICT(provider) DO UPDATE SET status = excluded.status, last_error_at = excluded.last_error_at,
            last_error_kind = excluded.last_error_kind, last_error = excluded.last_error,
            consecutive_failures = provider_health.consecutive_failures + 1, checked_at = excluded.checked_at",
    )
    .bind(provider)
    .bind(status)
    .bind(&t)
    .bind(kind)
    .bind(msg)
    .bind(&t)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_health(pool: &SqlitePool) -> R<Vec<HealthRow>> {
    sqlx::query_as("SELECT * FROM provider_health").fetch_all(pool).await
}

pub async fn get_health(pool: &SqlitePool, provider: &str) -> R<Option<HealthRow>> {
    sqlx::query_as("SELECT * FROM provider_health WHERE provider = ?").bind(provider).fetch_optional(pool).await
}

// -------------------------------------------------------------- retention

pub async fn prune_older_than(pool: &SqlitePool, cutoff: &str) -> R<u64> {
    let mut n = 0;
    for q in ["DELETE FROM observations WHERE observed_at < ?", "DELETE FROM events WHERE occurred_upper < ?", "DELETE FROM server_population WHERE t < ?"] {
        n += sqlx::query(q).bind(cutoff).execute(pool).await?.rows_affected();
    }
    Ok(n)
}

pub async fn purge_all(pool: &SqlitePool) -> R<()> {
    let mut tx = pool.begin().await?;
    for q in [
        "DELETE FROM events",
        "DELETE FROM observations",
        "DELETE FROM sessions",
        "DELETE FROM coverage_windows",
        "DELETE FROM tracked_players",
        "DELETE FROM squad_members",
        "DELETE FROM squads",
        "DELETE FROM aliases",
        "DELETE FROM bans",
        "DELETE FROM provider_links",
        "DELETE FROM server_population",
        "DELETE FROM servers",
        "DELETE FROM search_history",
        "DELETE FROM provider_health",
        "DELETE FROM players",
    ] {
        sqlx::query(q).execute(&mut *tx).await?;
    }
    tx.commit().await
}

pub async fn last_observation(pool: &SqlitePool, steam_id: &str, provider: &str) -> R<Option<ObservationRow>> {
    sqlx::query_as(
        "SELECT provider, observed_at, state, server_id, poll_interval_secs FROM observations
         WHERE steam_id = ? AND provider = ? ORDER BY observed_at DESC LIMIT 1",
    )
    .bind(steam_id)
    .bind(provider)
    .fetch_optional(pool)
    .await
}

pub async fn has_observations(pool: &SqlitePool, steam_id: &str) -> R<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT 1 FROM observations WHERE steam_id = ? LIMIT 1)").bind(steam_id).fetch_one(pool).await?;
    Ok(n > 0)
}

pub async fn table_counts(pool: &SqlitePool) -> R<Vec<(String, i64)>> {
    let mut out = Vec::new();
    for (name, q) in [
        ("players", "SELECT COUNT(*) FROM players"),
        ("sessions", "SELECT COUNT(*) FROM sessions"),
        ("observations", "SELECT COUNT(*) FROM observations"),
        ("coverage_windows", "SELECT COUNT(*) FROM coverage_windows"),
        ("events", "SELECT COUNT(*) FROM events"),
        ("servers", "SELECT COUNT(*) FROM servers"),
        ("squads", "SELECT COUNT(*) FROM squads"),
        ("tracked_players", "SELECT COUNT(*) FROM tracked_players"),
    ] {
        out.push((name.to_string(), sqlx::query_scalar(q).fetch_one(pool).await?));
    }
    Ok(out)
}

pub async fn upsert_server_stat(
    pool: &SqlitePool,
    steam_id: &str,
    st: &crate::providers::battlemetrics::BmServerStat,
    provider: &str,
    at: DateTime<Utc>,
) -> R<()> {
    sqlx::query(
        "INSERT INTO player_server_stats(steam_id, server_id, provider, server_name, online, first_seen, last_seen, time_played_secs, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(steam_id, server_id) DO UPDATE SET server_name = excluded.server_name, online = excluded.online,
            first_seen = COALESCE(excluded.first_seen, player_server_stats.first_seen), last_seen = COALESCE(excluded.last_seen, player_server_stats.last_seen),
            time_played_secs = COALESCE(excluded.time_played_secs, player_server_stats.time_played_secs), fetched_at = excluded.fetched_at",
    )
    .bind(steam_id)
    .bind(&st.server_id)
    .bind(provider)
    .bind(&st.server_name)
    .bind(st.online)
    .bind(st.first_seen.map(fmt_ts))
    .bind(st.last_seen.map(fmt_ts))
    .bind(st.time_played_secs)
    .bind(fmt_ts(at))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_server_stats(pool: &SqlitePool, steam_id: &str) -> R<Vec<ServerStatRow>> {
    sqlx::query_as(
        "SELECT server_id, provider, server_name, online, first_seen, last_seen, time_played_secs, fetched_at
         FROM player_server_stats WHERE steam_id = ? ORDER BY online DESC, last_seen DESC",
    )
    .bind(steam_id)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::db::open_memory;

    const SID: &str = "76561197960287930";

    #[tokio::test]
    async fn events_are_deduplicated() {
        let pool = open_memory().await.unwrap();
        ensure_player(&pool, SID, false).await.unwrap();
        let e = NewEvent {
            steam_id: Some(SID),
            provider: "steam",
            kind: "steam_in_game_started",
            dedup_key: "k1".into(),
            occurred_lower: now(),
            occurred_upper: now(),
            approximate: true,
            server_id: None,
            details: serde_json::json!({}),
        };
        assert!(insert_event(&pool, &e).await.unwrap().is_some());
        assert!(insert_event(&pool, &e).await.unwrap().is_none());
        assert_eq!(count_unread_events(&pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn observations_idempotent_and_cascade_delete() {
        let pool = open_memory().await.unwrap();
        ensure_player(&pool, SID, false).await.unwrap();
        let t = now();
        assert!(insert_observation(&pool, SID, "steam", t, "online", None, 120).await.unwrap());
        assert!(!insert_observation(&pool, SID, "steam", t, "online", None, 120).await.unwrap());
        delete_player(&pool, SID).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observations").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn coverage_windows_extend() {
        let pool = open_memory().await.unwrap();
        ensure_player(&pool, SID, false).await.unwrap();
        let t0 = crate::infrastructure::clock::parse_ts("2026-01-01T00:00:00Z").unwrap();
        let t1 = t0 + chrono::Duration::minutes(2);
        let t2 = t1 + chrono::Duration::minutes(2);
        add_coverage(&pool, SID, "steam", t0, t1).await.unwrap();
        add_coverage(&pool, SID, "steam", t1, t2).await.unwrap();
        let c = list_coverage(&pool, SID, "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await.unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].end_at, fmt_ts(t2));
    }

    #[tokio::test]
    async fn player_partial_update_keeps_fields() {
        let pool = open_memory().await.unwrap();
        upsert_player(&pool, &PlayerUpsert { steam_id: SID.into(), persona_name: Some("A".into()), ..Default::default() }).await.unwrap();
        upsert_player(&pool, &PlayerUpsert { steam_id: SID.into(), avatar_url: Some("x".into()), ..Default::default() }).await.unwrap();
        let p = get_player(&pool, SID).await.unwrap().unwrap();
        assert_eq!(p.persona_name.as_deref(), Some("A"));
        assert_eq!(p.avatar_url.as_deref(), Some("x"));
    }
}
