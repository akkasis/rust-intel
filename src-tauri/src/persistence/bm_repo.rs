//! Хранилище BattleMetrics: игроки (без обязательного SteamID), их серверы, сессии, наблюдаемые серверы.

use crate::infrastructure::clock::{fmt_ts, parse_ts};
use crate::providers::battlemetrics::BmServerStat;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::collections::{HashMap, HashSet};

type R<T> = Result<T, sqlx::Error>;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BmPlayerRow {
    pub bm_id: String,
    pub name: Option<String>,
    pub steam_id: Option<String>,
    pub tracked: bool,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub fetched_at: Option<String>,
    pub is_demo: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BmNameRow {
    pub name: String,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BmServerRow {
    pub server_id: String,
    pub server_name: Option<String>,
    pub online: bool,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub time_played_secs: Option<i64>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BmSessionRow {
    pub id: i64,
    pub bm_id: String,
    pub player_name: Option<String>,
    pub server_id: String,
    pub server_name: Option<String>,
    pub source: String,
    pub start_lower: String,
    pub start_upper: String,
    pub start_known: bool,
    pub end_lower: String,
    pub end_upper: Option<String>,
    pub end_known: bool,
    pub is_open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WatchRow {
    pub server_id: String,
    pub server_name: Option<String>,
    pub enabled: bool,
    pub added_at: String,
    pub last_polled_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub last_online: Option<i64>,
}

// ---------------------------------------------------------------- players

pub async fn upsert_player(pool: &SqlitePool, bm_id: &str, name: Option<&str>, seen: Option<DateTime<Utc>>, is_demo: bool) -> R<()> {
    let now = fmt_ts(Utc::now());
    let seen = seen.map(fmt_ts);
    sqlx::query(
        "INSERT INTO bm_players(bm_id, name, first_seen, last_seen, is_demo, updated_at) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(bm_id) DO UPDATE SET name = COALESCE(excluded.name, bm_players.name),
            first_seen = COALESCE(MIN(bm_players.first_seen, excluded.first_seen), bm_players.first_seen, excluded.first_seen),
            last_seen = COALESCE(MAX(bm_players.last_seen, excluded.last_seen), bm_players.last_seen, excluded.last_seen),
            updated_at = excluded.updated_at",
    )
    .bind(bm_id)
    .bind(name)
    .bind(&seen)
    .bind(&seen)
    .bind(is_demo)
    .bind(&now)
    .execute(pool)
    .await?;
    if let (Some(n), Some(s)) = (name, &seen) {
        record_name(pool, bm_id, n, s).await?;
    }
    Ok(())
}

pub async fn record_name(pool: &SqlitePool, bm_id: &str, name: &str, at: &str) -> R<()> {
    sqlx::query(
        "INSERT INTO bm_player_names(bm_id, name, first_seen, last_seen) VALUES (?, ?, ?, ?)
         ON CONFLICT(bm_id, name) DO UPDATE SET last_seen = MAX(last_seen, excluded.last_seen), first_seen = MIN(first_seen, excluded.first_seen)",
    )
    .bind(bm_id)
    .bind(name)
    .bind(at)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_fetched(pool: &SqlitePool, bm_id: &str, at: DateTime<Utc>) -> R<()> {
    sqlx::query("UPDATE bm_players SET fetched_at = ? WHERE bm_id = ?").bind(fmt_ts(at)).bind(bm_id).execute(pool).await?;
    Ok(())
}

pub async fn set_steam(pool: &SqlitePool, bm_id: &str, steam_id: Option<&str>) -> R<()> {
    sqlx::query("UPDATE bm_players SET steam_id = ? WHERE bm_id = ?").bind(steam_id).bind(bm_id).execute(pool).await?;
    Ok(())
}

pub async fn set_tracked(pool: &SqlitePool, bm_id: &str, tracked: bool) -> R<()> {
    sqlx::query("UPDATE bm_players SET tracked = ? WHERE bm_id = ?").bind(tracked).bind(bm_id).execute(pool).await?;
    Ok(())
}

pub async fn get_player(pool: &SqlitePool, bm_id: &str) -> R<Option<BmPlayerRow>> {
    sqlx::query_as("SELECT * FROM bm_players WHERE bm_id = ?").bind(bm_id).fetch_optional(pool).await
}

pub async fn list_players(pool: &SqlitePool, query: Option<&str>, tracked_only: bool, limit: i64) -> R<Vec<BmPlayerRow>> {
    let like = query.filter(|q| !q.trim().is_empty()).map(|q| format!("%{}%", q.trim()));
    sqlx::query_as(
        "SELECT * FROM bm_players WHERE (? IS NULL OR name LIKE ? OR bm_id = ?) AND (? = 0 OR tracked = 1)
         ORDER BY tracked DESC, last_seen DESC LIMIT ?",
    )
    .bind(&like)
    .bind(&like)
    .bind(query.map(str::trim))
    .bind(tracked_only)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn list_names(pool: &SqlitePool, bm_id: &str) -> R<Vec<BmNameRow>> {
    sqlx::query_as("SELECT name, first_seen, last_seen FROM bm_player_names WHERE bm_id = ? ORDER BY last_seen DESC").bind(bm_id).fetch_all(pool).await
}

pub async fn upsert_player_server(pool: &SqlitePool, bm_id: &str, st: &BmServerStat, at: DateTime<Utc>) -> R<()> {
    sqlx::query(
        "INSERT INTO bm_player_servers(bm_id, server_id, server_name, online, first_seen, last_seen, time_played_secs, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(bm_id, server_id) DO UPDATE SET server_name = excluded.server_name, online = excluded.online,
            first_seen = COALESCE(excluded.first_seen, bm_player_servers.first_seen), last_seen = COALESCE(excluded.last_seen, bm_player_servers.last_seen),
            time_played_secs = COALESCE(excluded.time_played_secs, bm_player_servers.time_played_secs), fetched_at = excluded.fetched_at",
    )
    .bind(bm_id)
    .bind(&st.server_id)
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

pub async fn list_player_servers(pool: &SqlitePool, bm_id: &str) -> R<Vec<BmServerRow>> {
    sqlx::query_as(
        "SELECT server_id, server_name, online, first_seen, last_seen, time_played_secs, fetched_at FROM bm_player_servers
         WHERE bm_id = ? ORDER BY online DESC, last_seen DESC",
    )
    .bind(bm_id)
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------- sessions

pub async fn list_sessions_player(pool: &SqlitePool, bm_id: &str, from: &str, to: &str, limit: i64) -> R<Vec<BmSessionRow>> {
    sqlx::query_as(
        "SELECT s.id, s.bm_id, p.name AS player_name, s.server_id, sv.name AS server_name, s.source, s.start_lower, s.start_upper,
                s.start_known, s.end_lower, s.end_upper, s.end_known, s.is_open
         FROM bm_sessions s JOIN bm_players p ON p.bm_id = s.bm_id LEFT JOIN servers sv ON sv.id = s.server_id
         WHERE s.bm_id = ? AND s.end_lower >= ? AND s.start_lower <= ? ORDER BY s.start_upper DESC LIMIT ?",
    )
    .bind(bm_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Сессии всех игроков на заданном сервере за период (для поиска тиммейтов).
pub async fn list_sessions_server(pool: &SqlitePool, server_id: &str, from: &str, to: &str) -> R<Vec<BmSessionRow>> {
    sqlx::query_as(
        "SELECT s.id, s.bm_id, p.name AS player_name, s.server_id, NULL AS server_name, s.source, s.start_lower, s.start_upper,
                s.start_known, s.end_lower, s.end_upper, s.end_known, s.is_open
         FROM bm_sessions s JOIN bm_players p ON p.bm_id = s.bm_id
         WHERE s.server_id = ? AND s.end_lower >= ? AND s.start_lower <= ?",
    )
    .bind(server_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await
}

pub async fn open_sessions_server(pool: &SqlitePool, server_id: &str) -> R<Vec<BmSessionRow>> {
    sqlx::query_as(
        "SELECT s.id, s.bm_id, p.name AS player_name, s.server_id, NULL AS server_name, s.source, s.start_lower, s.start_upper,
                s.start_known, s.end_lower, s.end_upper, s.end_known, s.is_open
         FROM bm_sessions s JOIN bm_players p ON p.bm_id = s.bm_id
         WHERE s.server_id = ? AND s.is_open = 1 ORDER BY s.start_upper",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await
}

pub struct NewBmSession<'a> {
    pub bm_id: &'a str,
    pub server_id: &'a str,
    pub source: &'a str,
    pub start_lower: DateTime<Utc>,
    pub start_upper: DateTime<Utc>,
    pub start_known: bool,
    pub end_lower: DateTime<Utc>,
    pub end_upper: Option<DateTime<Utc>>,
    pub end_known: bool,
    pub is_open: bool,
    pub external_id: Option<&'a str>,
}

pub async fn upsert_session(pool: &SqlitePool, s: &NewBmSession<'_>) -> R<()> {
    sqlx::query(
        "INSERT INTO bm_sessions(bm_id, server_id, source, start_lower, start_upper, start_known, end_lower, end_upper, end_known, is_open, external_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(bm_id, server_id, start_upper) DO UPDATE SET end_lower = MAX(bm_sessions.end_lower, excluded.end_lower),
            end_upper = excluded.end_upper, end_known = excluded.end_known, is_open = excluded.is_open",
    )
    .bind(s.bm_id)
    .bind(s.server_id)
    .bind(s.source)
    .bind(fmt_ts(s.start_lower))
    .bind(fmt_ts(s.start_upper))
    .bind(s.start_known)
    .bind(fmt_ts(s.end_lower))
    .bind(s.end_upper.map(fmt_ts))
    .bind(s.end_known)
    .bind(s.is_open)
    .bind(s.external_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Результат применения снимка онлайна сервера.
#[derive(Debug, Default, Clone, Serialize)]
pub struct RosterDelta {
    pub joined: Vec<(String, String)>,
    pub left: Vec<(String, String)>,
    pub online: usize,
    pub contiguous: bool,
}

/// Применяет снимок «кто онлайн» к сессиям сервера (одной транзакцией).
/// `roster = None` — опрос не удался: открытые сессии закрываются с неизвестным концом, покрытие прерывается.
pub async fn apply_roster(
    pool: &SqlitePool,
    server_id: &str,
    roster: Option<&[(String, String)]>,
    t: DateTime<Utc>,
    interval_secs: i64,
    is_demo: bool,
) -> R<RosterDelta> {
    let mut tx = pool.begin().await?;
    let watch: Option<(Option<String>,)> =
        sqlx::query_as("SELECT last_success_at FROM server_watch WHERE server_id = ?").bind(server_id).fetch_optional(&mut *tx).await?;
    let prev = watch.and_then(|w| w.0).as_deref().and_then(parse_ts);
    let contiguous = prev.is_some_and(|p| t > p && t - p <= Duration::seconds((interval_secs as f64 * 2.5) as i64));
    let ts = fmt_ts(t);
    let open: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT s.id, s.bm_id, COALESCE(p.name, s.bm_id), s.start_upper FROM bm_sessions s JOIN bm_players p ON p.bm_id = s.bm_id
         WHERE s.server_id = ? AND s.is_open = 1",
    )
    .bind(server_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut delta = RosterDelta { contiguous, ..Default::default() };
    let Some(roster) = roster else {
        sqlx::query("UPDATE bm_sessions SET is_open = 0, end_upper = NULL, end_known = 0 WHERE server_id = ? AND is_open = 1")
            .bind(server_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE server_watch SET last_polled_at = ? WHERE server_id = ?").bind(&ts).bind(server_id).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(delta);
    };
    delta.online = roster.len();
    let open_by: HashMap<&str, &(i64, String, String, String)> = open.iter().map(|o| (o.1.as_str(), o)).collect();
    let now_ids: HashSet<&str> = roster.iter().map(|r| r.0.as_str()).collect();
    for (bm_id, name) in roster {
        sqlx::query(
            "INSERT INTO bm_players(bm_id, name, first_seen, last_seen, is_demo, updated_at) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(bm_id) DO UPDATE SET name = excluded.name, last_seen = excluded.last_seen, updated_at = excluded.updated_at",
        )
        .bind(bm_id)
        .bind(name)
        .bind(&ts)
        .bind(&ts)
        .bind(is_demo)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO bm_player_names(bm_id, name, first_seen, last_seen) VALUES (?, ?, ?, ?)
             ON CONFLICT(bm_id, name) DO UPDATE SET last_seen = excluded.last_seen",
        )
        .bind(bm_id)
        .bind(name)
        .bind(&ts)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
        match (open_by.get(bm_id.as_str()), contiguous) {
            (Some(o), true) => {
                sqlx::query("UPDATE bm_sessions SET end_lower = ? WHERE id = ?").bind(&ts).bind(o.0).execute(&mut *tx).await?;
            }
            (existing, _) => {
                if let Some(o) = existing {
                    // пробел в наблюдении: старая сессия закрывается с неизвестным концом
                    sqlx::query("UPDATE bm_sessions SET is_open = 0, end_upper = NULL, end_known = 0 WHERE id = ?").bind(o.0).execute(&mut *tx).await?;
                }
                let start_lower = if contiguous { prev.map(fmt_ts).unwrap_or(ts.clone()) } else { ts.clone() };
                sqlx::query(
                    "INSERT OR IGNORE INTO bm_sessions(bm_id, server_id, source, start_lower, start_upper, start_known, end_lower, end_upper, end_known, is_open)
                     VALUES (?, ?, 'server_watch', ?, ?, ?, ?, NULL, 0, 1)",
                )
                .bind(bm_id)
                .bind(server_id)
                .bind(&start_lower)
                .bind(&ts)
                .bind(contiguous)
                .bind(&ts)
                .execute(&mut *tx)
                .await?;
                if contiguous {
                    delta.joined.push((bm_id.clone(), name.clone()));
                }
            }
        }
    }
    for o in &open {
        if now_ids.contains(o.1.as_str()) {
            continue;
        }
        if contiguous {
            sqlx::query("UPDATE bm_sessions SET is_open = 0, end_upper = ?, end_known = 1 WHERE id = ?").bind(&ts).bind(o.0).execute(&mut *tx).await?;
            delta.left.push((o.1.clone(), o.2.clone()));
        } else {
            sqlx::query("UPDATE bm_sessions SET is_open = 0, end_upper = NULL, end_known = 0 WHERE id = ?").bind(o.0).execute(&mut *tx).await?;
        }
    }
    if contiguous && let Some(p) = prev {
        let ps = fmt_ts(p);
        let r = sqlx::query("UPDATE server_coverage SET end_at = ? WHERE server_id = ? AND end_at = ?")
            .bind(&ts)
            .bind(server_id)
            .bind(&ps)
            .execute(&mut *tx)
            .await?;
        if r.rows_affected() == 0 {
            sqlx::query("INSERT OR IGNORE INTO server_coverage(server_id, start_at, end_at) VALUES (?, ?, ?)")
                .bind(server_id)
                .bind(&ps)
                .bind(&ts)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query(
        "INSERT INTO server_watch(server_id, enabled, added_at, last_polled_at, last_success_at, last_online) VALUES (?, 1, ?, ?, ?, ?)
         ON CONFLICT(server_id) DO UPDATE SET last_polled_at = excluded.last_polled_at, last_success_at = excluded.last_success_at,
            last_error = NULL, last_online = excluded.last_online",
    )
    .bind(server_id)
    .bind(&ts)
    .bind(&ts)
    .bind(&ts)
    .bind(roster.len() as i64)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(delta)
}

// ---------------------------------------------------------------- watch / coverage

pub async fn set_watch(pool: &SqlitePool, server_id: &str, enabled: bool) -> R<()> {
    sqlx::query(
        "INSERT INTO server_watch(server_id, enabled, added_at) VALUES (?, ?, ?)
         ON CONFLICT(server_id) DO UPDATE SET enabled = excluded.enabled",
    )
    .bind(server_id)
    .bind(enabled)
    .bind(fmt_ts(Utc::now()))
    .execute(pool)
    .await?;
    if !enabled {
        // наблюдение прекращено: открытые сессии на сервере больше не продлеваются
        sqlx::query("UPDATE bm_sessions SET is_open = 0, end_upper = NULL, end_known = 0 WHERE server_id = ? AND is_open = 1 AND source = 'server_watch'")
            .bind(server_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn record_watch_error(pool: &SqlitePool, server_id: &str, err: &str) -> R<()> {
    sqlx::query("UPDATE server_watch SET last_polled_at = ?, last_error = ? WHERE server_id = ?")
        .bind(fmt_ts(Utc::now()))
        .bind(err)
        .bind(server_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_watch(pool: &SqlitePool) -> R<Vec<WatchRow>> {
    sqlx::query_as(
        "SELECT w.server_id, s.name AS server_name, w.enabled, w.added_at, w.last_polled_at, w.last_success_at, w.last_error, w.last_online
         FROM server_watch w LEFT JOIN servers s ON s.id = w.server_id ORDER BY w.enabled DESC, s.name",
    )
    .fetch_all(pool)
    .await
}

pub async fn list_coverage_all(pool: &SqlitePool, from: &str, to: &str) -> R<Vec<(String, String)>> {
    sqlx::query_as("SELECT start_at, end_at FROM server_coverage WHERE end_at >= ? AND start_at <= ?").bind(from).bind(to).fetch_all(pool).await
}

pub async fn add_coverage(pool: &SqlitePool, server_id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> R<()> {
    sqlx::query("INSERT OR IGNORE INTO server_coverage(server_id, start_at, end_at) VALUES (?, ?, ?)")
        .bind(server_id)
        .bind(fmt_ts(start))
        .bind(fmt_ts(end))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn count_players(pool: &SqlitePool) -> R<(i64, i64)> {
    sqlx::query_as("SELECT (SELECT COUNT(*) FROM bm_players), (SELECT COUNT(*) FROM bm_sessions)").fetch_one(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::db::open_memory;

    fn r(ids: &[&str]) -> Vec<(String, String)> {
        ids.iter().map(|i| (i.to_string(), format!("N{i}"))).collect()
    }

    #[tokio::test]
    async fn roster_polls_build_sessions_with_known_bounds() {
        let pool = open_memory().await.unwrap();
        set_watch(&pool, "battlemetrics:1", true).await.unwrap();
        let t0 = parse_ts("2026-11-01T18:00:00Z").unwrap();
        let st = Duration::seconds(120);
        // первый опрос: A уже онлайн — начало неизвестно, событий нет
        let d = apply_roster(&pool, "battlemetrics:1", Some(&r(&["1"])), t0, 120, false).await.unwrap();
        assert!(!d.contiguous && d.joined.is_empty());
        // B зашёл
        let d = apply_roster(&pool, "battlemetrics:1", Some(&r(&["1", "2"])), t0 + st, 120, false).await.unwrap();
        assert_eq!(d.joined, vec![("2".to_string(), "N2".to_string())]);
        // A вышел
        let d = apply_roster(&pool, "battlemetrics:1", Some(&r(&["2"])), t0 + st * 2, 120, false).await.unwrap();
        assert_eq!(d.left.len(), 1);
        // сбой опроса — B не «вышел», конец неизвестен
        let d = apply_roster(&pool, "battlemetrics:1", None, t0 + st * 3, 120, false).await.unwrap();
        assert!(d.left.is_empty());
        let a = list_sessions_player(&pool, "1", "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z", 10).await.unwrap();
        assert!(!a[0].start_known && a[0].end_known);
        let b = list_sessions_player(&pool, "2", "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z", 10).await.unwrap();
        assert!(b[0].start_known && !b[0].end_known && !b[0].is_open);
        let cov = list_coverage_all(&pool, "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await.unwrap();
        assert_eq!(cov, vec![(fmt_ts(t0), fmt_ts(t0 + st * 2))]);
    }

    #[tokio::test]
    async fn long_gap_does_not_create_join_events() {
        let pool = open_memory().await.unwrap();
        set_watch(&pool, "s", true).await.unwrap();
        let t0 = parse_ts("2026-11-01T18:00:00Z").unwrap();
        apply_roster(&pool, "s", Some(&r(&["1"])), t0, 120, false).await.unwrap();
        let d = apply_roster(&pool, "s", Some(&r(&["1", "2"])), t0 + Duration::hours(5), 120, false).await.unwrap();
        assert!(!d.contiguous && d.joined.is_empty() && d.left.is_empty());
    }
}
