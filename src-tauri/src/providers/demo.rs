//! Демо-режим: ВЫМЫШЛЕННЫЕ записи для оценки интерфейса без ключей.
//! Демо хранится в отдельной БД (rid-demo.sqlite) и никогда не смешивается с реальными данными.
//! SteamID демо-профилей лежат в диапазоне account id ≥ 4 100 000 000, который Steam ещё не выдавал;
//! ссылок на steamcommunity.com для них нет.

use super::error::ProviderError;
use super::{Presence, PresenceSource, PresenceState};
use crate::domain::bans::{BanKind, BanRecord};
use crate::domain::ids::{STEAM_ID64_BASE, SteamId64};
use crate::infrastructure::clock::fmt_ts;
use crate::persistence::models::{ProviderLinkRow, ServerRow};
use crate::persistence::repo::{self, NewEvent, NewSession, PlayerUpsert};
use async_trait::async_trait;
use chrono::{DateTime, Duration, DurationRound, Timelike, Utc};
use sqlx::SqlitePool;

pub const DEMO_ACCOUNT_BASE: u64 = 4_100_000_000;
const POLL_MIN: i64 = 5;

pub fn demo_steam_id(n: u64) -> SteamId64 {
    SteamId64::from_u64(STEAM_ID64_BASE + DEMO_ACCOUNT_BASE + n).expect("demo id")
}

pub fn is_demo_id(id: &SteamId64) -> bool {
    id.account_id() as u64 >= DEMO_ACCOUNT_BASE
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }
    fn range(&mut self, a: i64, b: i64) -> i64 {
        a + (self.next() % ((b - a).max(1) as u64)) as i64
    }
}

pub struct DemoPlayer {
    pub n: u64,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub group: usize,
    /// Предпочитаемое начало игры, час UTC.
    pub start_hour_utc: i64,
    pub play_prob: f64,
    pub private: bool,
    pub hours: Option<f64>,
    pub vac: i64,
    pub game: i64,
    pub days_since_ban: i64,
    pub created_days_ago: Option<i64>,
    /// Дней наблюдения (меньше 90 → показывает недостаток данных).
    pub observed_days: i64,
}

pub const PLAYERS: &[DemoPlayer] = &[
    DemoPlayer {
        n: 1,
        name: "DEMO·Ворон",
        aliases: &["DEMO·Raven", "DEMO·ворон_77"],
        group: 0,
        start_hour_utc: 16,
        play_prob: 0.8,
        private: false,
        hours: Some(3120.0),
        vac: 0,
        game: 0,
        days_since_ban: 0,
        created_days_ago: Some(3200),
        observed_days: 90,
    },
    DemoPlayer {
        n: 2,
        name: "DEMO·Клещ",
        aliases: &["DEMO·Tick"],
        group: 0,
        start_hour_utc: 16,
        play_prob: 0.75,
        private: false,
        hours: Some(1850.0),
        vac: 0,
        game: 1,
        days_since_ban: 410,
        created_days_ago: Some(1500),
        observed_days: 90,
    },
    DemoPlayer {
        n: 3,
        name: "DEMO·Сова",
        aliases: &[],
        group: 0,
        start_hour_utc: 17,
        play_prob: 0.6,
        private: true,
        hours: None,
        vac: 0,
        game: 0,
        days_since_ban: 0,
        created_days_ago: None,
        observed_days: 90,
    },
    DemoPlayer {
        n: 4,
        name: "DEMO·Бетон",
        aliases: &["DEMO·Concrete", "DEMO·бетонщик"],
        group: 1,
        start_hour_utc: 19,
        play_prob: 0.7,
        private: false,
        hours: Some(5400.0),
        vac: 1,
        game: 0,
        days_since_ban: 1200,
        created_days_ago: Some(4000),
        observed_days: 90,
    },
    DemoPlayer {
        n: 5,
        name: "DEMO·Лис",
        aliases: &[],
        group: 1,
        start_hour_utc: 19,
        play_prob: 0.65,
        private: false,
        hours: Some(980.0),
        vac: 0,
        game: 0,
        days_since_ban: 0,
        created_days_ago: Some(700),
        observed_days: 90,
    },
    DemoPlayer {
        n: 6,
        name: "DEMO·Новичок",
        aliases: &[],
        group: 2,
        start_hour_utc: 12,
        play_prob: 0.5,
        private: false,
        hours: Some(42.0),
        vac: 0,
        game: 0,
        days_since_ban: 0,
        created_days_ago: Some(40),
        observed_days: 6,
    },
    DemoPlayer {
        n: 7,
        name: "DEMO·Полуночник",
        aliases: &["DEMO·Midnight"],
        group: 3,
        start_hour_utc: 22,
        play_prob: 0.85,
        private: false,
        hours: Some(7800.0),
        vac: 0,
        game: 2,
        days_since_ban: 60,
        created_days_ago: Some(2900),
        observed_days: 90,
    },
    DemoPlayer {
        n: 8,
        name: "DEMO·Тихий",
        aliases: &[],
        group: 4,
        start_hour_utc: 8,
        play_prob: 0.4,
        private: false,
        hours: None,
        vac: 0,
        game: 0,
        days_since_ban: 0,
        created_days_ago: Some(5000),
        observed_days: 45,
    },
];

pub const SERVERS: &[(&str, &str, &str, i64)] = &[
    ("1", "[DEMO] Северный Форпост 2x", "RU", 200),
    ("2", "[DEMO] Ржавая Гавань Vanilla", "DE", 300),
    ("3", "[DEMO] Пустошь Solo/Duo", "FI", 150),
    ("4", "[DEMO] Ночной Рейд 5x", "PL", 250),
];

fn group_server(group: usize) -> &'static str {
    match group {
        0 => "1",
        1 => "2",
        2 => "3",
        _ => "4",
    }
}

fn floor5(t: DateTime<Utc>) -> DateTime<Utc> {
    t.duration_trunc(Duration::minutes(POLL_MIN)).unwrap_or(t)
}

/// Заполняет демо-БД. Идемпотентно: при повторном вызове не дублирует записи.
pub async fn seed(pool: &SqlitePool, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    if repo::get_setting(pool, "demo_seeded").await?.is_none() {
        seed_base(pool, now).await?;
    }
    // Слой BattleMetrics добавлен в 0.3 — досеивается и в уже существующие демо-базы.
    if repo::get_setting(pool, "demo_bm_seeded").await?.is_none() {
        seed_bm(pool, now).await?;
    }
    if repo::get_setting(pool, "demo_v4_seeded").await?.is_none() {
        seed_v4(pool, now).await?;
    }
    Ok(())
}

/// 0.4: вайпы по четвергам, клан BM и вымышленный «новый аккаунт» рядом с группой Ворона.
async fn seed_v4(pool: &SqlitePool, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    use crate::persistence::bm_repo::{self, NewBmSession};
    let now = floor5(now);
    let mut rng = Rng(0x0a17_5eed_4444_0001);
    for (id, ..) in SERVERS {
        let sid = format!("demo:{id}");
        let mut d = (now - Duration::days(91)).duration_trunc(Duration::days(1)).unwrap_or(now) + Duration::hours(16);
        let mut last = None;
        while d < now {
            if d.format("%u").to_string() == "4" {
                crate::services::insights_service::record_wipe(pool, &sid, &fmt_ts(d), "demo").await?;
                last = Some(d);
            }
            d += Duration::days(1);
        }
        if let Some(l) = last {
            sqlx::query("UPDATE servers SET last_wipe_at = ? WHERE id = ?").bind(fmt_ts(l)).bind(&sid).execute(pool).await?;
        }
    }
    // «новый аккаунт»: появился 12 дней назад и заходит вместе с Клещом
    bm_repo::upsert_player(pool, "9200003", Some("DEMO·Новый_акк"), None, true).await?;
    let klesh = bm_repo::list_sessions_player(pool, &demo_bm_id(2), &fmt_ts(now - Duration::days(12)), &fmt_ts(now), 500).await?;
    for s in klesh {
        if rng.f() > 0.85 {
            continue;
        }
        let (Some(st), Some(el)) = (crate::infrastructure::clock::parse_ts(&s.start_upper), crate::infrastructure::clock::parse_ts(&s.end_lower)) else {
            continue;
        };
        let st2 = st + Duration::minutes(rng.range(-1, 2) * 5);
        let en2 = el + Duration::minutes(rng.range(-1, 2) * 5);
        bm_repo::upsert_session(
            pool,
            &NewBmSession {
                bm_id: "9200003",
                server_id: &s.server_id,
                source: "demo",
                start_lower: st2 - Duration::minutes(POLL_MIN),
                start_upper: st2,
                start_known: s.start_known,
                end_lower: if s.is_open { now } else { en2 },
                end_upper: if s.is_open { None } else { Some(en2 + Duration::minutes(POLL_MIN)) },
                end_known: s.end_known,
                is_open: s.is_open,
                external_id: None,
            },
        )
        .await?;
    }
    sqlx::query("UPDATE bm_players SET first_seen = (SELECT MIN(start_upper) FROM bm_sessions WHERE bm_id = '9200003'), last_seen = (SELECT MAX(end_lower) FROM bm_sessions WHERE bm_id = '9200003'), fetched_at = ? WHERE bm_id = '9200003'")
        .bind(fmt_ts(now))
        .execute(pool)
        .await?;
    let ts = fmt_ts(now);
    let clan =
        sqlx::query("INSERT INTO bm_clans(name, color, notify, min_gather, created_at, updated_at) VALUES ('[DEMO] Северяне (BM)', '#a3e635', 1, 2, ?, ?)")
            .bind(&ts)
            .bind(&ts)
            .execute(pool)
            .await?
            .last_insert_rowid();
    for (bm, prov) in [(demo_bm_id(1), "demo"), (demo_bm_id(2), "demo"), (demo_bm_id(3), "demo"), ("9200001".to_string(), "teammate_suggestion")] {
        sqlx::query("INSERT OR IGNORE INTO bm_clan_members(clan_id, bm_id, provenance, added_at) VALUES (?, ?, ?, ?)")
            .bind(clan)
            .bind(bm)
            .bind(prov)
            .bind(&ts)
            .execute(pool)
            .await?;
    }
    repo::set_setting(pool, "demo_v4_seeded", &ts).await?;
    Ok(())
}

pub fn demo_bm_id(n: u64) -> String {
    format!("{}", 9_100_000 + n)
}

/// Вымышленные игроки BattleMetrics: копии демо-игроков, «тиммейт» Ворона, «попутчик» и массовка.
async fn seed_bm(pool: &SqlitePool, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    use crate::persistence::bm_repo::{self, NewBmSession};
    let now = floor5(now);
    let mut rng = Rng(0xb33f_5eed_0042_1337);
    let p = crate::infrastructure::clock::parse_ts;
    let lo = "0000-01-01T00:00:00Z";
    let hi = fmt_ts(now + Duration::days(1));
    // покрытие серверов = окна, когда «приложение» работало
    let windows: Vec<(DateTime<Utc>, DateTime<Utc>)> =
        repo::list_coverage(pool, demo_steam_id(1).as_str(), lo, &hi).await?.iter().filter_map(|c| Some((p(&c.start_at)?, p(&c.end_at)?))).collect();
    for (id, ..) in SERVERS {
        let sid = format!("demo:{id}");
        for (a, b) in &windows {
            bm_repo::add_coverage(pool, &sid, *a, *b).await?;
        }
        if *id == "1" || *id == "2" {
            bm_repo::set_watch(pool, &sid, true).await?;
            sqlx::query("UPDATE server_watch SET last_polled_at = ?, last_success_at = ?, last_online = ? WHERE server_id = ?")
                .bind(fmt_ts(now))
                .bind(fmt_ts(now))
                .bind(rng.range(40, 180))
                .bind(&sid)
                .execute(pool)
                .await?;
        }
    }
    let in_window = |t: DateTime<Utc>| windows.iter().find(|(a, b)| t >= *a && t < *b).map(|w| w.1);
    let mut sessions: Vec<(String, String, DateTime<Utc>, DateTime<Utc>)> = vec![];
    // 1) демо-игроки → игроки BM, их сессии копируются
    for pl in PLAYERS {
        let bm = demo_bm_id(pl.n);
        let steam = demo_steam_id(pl.n);
        bm_repo::upsert_player(pool, &bm, Some(pl.name), None, true).await?;
        bm_repo::set_steam(pool, &bm, Some(steam.as_str())).await?;
        if pl.n == 1 || pl.n == 7 {
            bm_repo::set_tracked(pool, &bm, true).await?;
        }
        for a in pl.aliases {
            bm_repo::record_name(pool, &bm, a, &fmt_ts(now - Duration::days(40))).await?;
        }
        for se in repo::list_sessions(pool, steam.as_str(), lo, &hi).await? {
            let (Some(st), Some(el), Some(server)) = (p(&se.start_upper), p(&se.end_lower), se.server_id.clone()) else { continue };
            let en = if se.is_open { now + Duration::minutes(1) } else { el };
            sessions.push((bm.clone(), server, st, en));
        }
    }
    // 2) «тиммейт» Ворона: входит/выходит вместе (±5 мин) в ~75% его сессий
    let voron: Vec<_> = sessions.iter().filter(|s| s.0 == demo_bm_id(1)).cloned().collect();
    bm_repo::upsert_player(pool, "9200001", Some("DEMO·Сосед"), None, true).await?;
    bm_repo::record_name(pool, "9200001", "DEMO·neighbour", &fmt_ts(now - Duration::days(60))).await?;
    bm_repo::upsert_player(pool, "9200002", Some("DEMO·Попутчик"), None, true).await?;
    for (_, server, st, en) in &voron {
        if rng.f() < 0.75 {
            let s2 = *st + Duration::minutes(rng.range(-5, 6));
            let e2 = if *en > now { *en } else { *en + Duration::minutes(rng.range(-6, 7)) };
            sessions.push(("9200001".into(), server.clone(), s2, e2));
        }
        // «попутчик» часто на том же сервере, но заходит/выходит в другое время
        if rng.f() < 0.6 {
            let s3 = *st + Duration::minutes(rng.range(35, 90));
            sessions.push(("9200002".into(), server.clone(), s3, s3 + Duration::minutes(rng.range(60, 240))));
        }
    }
    // 3) массовка на серверах
    for k in 1..=40u64 {
        let bm = format!("{}", 9_300_000 + k);
        bm_repo::upsert_player(pool, &bm, Some(&format!("DEMO·Игрок_{k:02}")), None, true).await?;
        let server = format!("demo:{}", SERVERS[(k % 4) as usize].0);
        for d in (0..90).rev() {
            if rng.f() > 0.45 {
                continue;
            }
            let day = (now - Duration::days(d)).duration_trunc(Duration::days(1)).unwrap_or(now);
            let st = day + Duration::minutes(rng.range(8 * 60, 23 * 60));
            sessions.push((bm.clone(), server.clone(), st, st + Duration::minutes(rng.range(30, 300))));
        }
    }
    // запись: только внутри окон наблюдения, с квантованием опроса
    for (bm, server, st, en) in sessions {
        if st >= now {
            continue;
        }
        let Some(win_end) = in_window(st) else { continue };
        let open = en >= now;
        let (end_lower, end_upper, end_known) = if open {
            (now, None, false)
        } else if en >= win_end {
            (floor5(win_end), None, false)
        } else {
            (floor5(en), Some(floor5(en) + Duration::minutes(POLL_MIN)), true)
        };
        let start_upper = floor5(st) + Duration::minutes(POLL_MIN);
        if end_lower <= start_upper {
            continue;
        }
        bm_repo::upsert_session(
            pool,
            &NewBmSession {
                bm_id: &bm,
                server_id: &server,
                source: "demo",
                start_lower: floor5(st),
                start_upper,
                start_known: true,
                end_lower,
                end_upper,
                end_known,
                is_open: open,
                external_id: None,
            },
        )
        .await?;
    }
    // серверная статистика игроков BM из их сессий
    sqlx::query(
        "INSERT OR REPLACE INTO bm_player_servers(bm_id, server_id, server_name, online, first_seen, last_seen, time_played_secs, fetched_at)
         SELECT s.bm_id, s.server_id, sv.name, MAX(s.is_open), MIN(s.start_upper), MAX(s.end_lower),
                SUM(strftime('%s', s.end_lower) - strftime('%s', s.start_upper)), ?
         FROM bm_sessions s LEFT JOIN servers sv ON sv.id = s.server_id GROUP BY s.bm_id, s.server_id",
    )
    .bind(fmt_ts(now))
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE bm_players SET first_seen = (SELECT MIN(start_upper) FROM bm_sessions s WHERE s.bm_id = bm_players.bm_id),
                               last_seen = (SELECT MAX(end_lower) FROM bm_sessions s WHERE s.bm_id = bm_players.bm_id),
                               fetched_at = ?",
    )
    .bind(fmt_ts(now))
    .execute(pool)
    .await?;
    repo::set_setting(pool, "demo_bm_seeded", &fmt_ts(now)).await?;
    Ok(())
}

async fn seed_base(pool: &SqlitePool, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    let now = floor5(now);
    let mut rng = Rng(0x5eed_1234_abcd_ef01);

    for (id, name, country, max) in SERVERS {
        repo::upsert_server(
            pool,
            &ServerRow {
                id: format!("demo:{id}"),
                provider: "demo".into(),
                external_id: (*id).into(),
                name: (*name).into(),
                address: Some(format!("198.51.100.{id}:28015")),
                country: Some((*country).into()),
                map: Some("Procedural Map".into()),
                players: Some(rng.range(*max / 3, *max)),
                max_players: Some(*max),
                queue: Some(rng.range(0, 5)),
                online: Some(true),
                trust: Some("demo".into()),
                last_wipe_at: Some(fmt_ts(now - Duration::days(rng.range(1, 6)))),
                next_wipe_at: Some(fmt_ts(now + Duration::days(rng.range(1, 6)))),
                followed: *id == "1" || *id == "2",
                source_url: None,
                fetched_at: fmt_ts(now),
                is_demo: true,
            },
        )
        .await?;
        // часовая история онлайна сервера за 30 дней (серверный уровень)
        let mut pts = Vec::new();
        for h in 0..(30 * 24) {
            let t = now - Duration::hours(h);
            let hour = t.hour() as f64;
            let wave = ((hour - 18.0) / 24.0 * std::f64::consts::TAU).cos();
            let p = (*max as f64 * (0.45 + 0.35 * wave) + rng.f() * 15.0).clamp(0.0, *max as f64);
            pts.push((fmt_ts(t.duration_trunc(Duration::hours(1)).unwrap_or(t)), (p * 10.0).round() / 10.0));
        }
        repo::insert_population(pool, &format!("demo:{id}"), &pts, "demo").await?;
    }

    // Окна работы «приложения»: каждый день с перерывами, плюс трёхдневный пробел.
    let mut app_windows: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for d in (0..90).rev() {
        if (40..43).contains(&d) {
            continue; // ПК был выключен
        }
        let day = (now - Duration::days(d)).duration_trunc(Duration::days(1)).unwrap_or(now);
        let off_start = rng.range(1, 6);
        let off_len = rng.range(0, 7);
        let a = day;
        let b = day + Duration::hours(off_start);
        let c = b + Duration::hours(off_len);
        let e = (day + Duration::days(1)).min(now);
        if b > a {
            app_windows.push((a, b.min(e)));
        }
        if e > c {
            app_windows.push((c, e));
        }
    }

    for p in PLAYERS {
        let sid = demo_steam_id(p.n);
        let s = sid.as_str();
        repo::upsert_player(
            pool,
            &PlayerUpsert {
                steam_id: s.into(),
                persona_name: Some(p.name.into()),
                visibility: Some(if p.private { "private" } else { "public" }.into()),
                account_created_at: p.created_days_ago.map(|d| fmt_ts(now - Duration::days(d))),
                country_code: Some("ZZ".into()),
                rust_hours: p.hours,
                rust_hours_status: Some(if p.hours.is_some() { "verified" } else { "hidden" }.into()),
                steam_in_game: Some(false),
                steam_status: Some("demo".into()),
                steam_fetched_at: Some(fmt_ts(now - Duration::minutes(rng.range(5, 300)))),
                is_demo: true,
                ..Default::default()
            },
        )
        .await?;
        if p.n <= 2 || p.n == 7 {
            repo::set_favorite(pool, s, true).await?;
        }
        repo::record_alias(pool, s, p.name, "demo", now - Duration::days(2)).await?;
        for (i, a) in p.aliases.iter().enumerate() {
            repo::record_alias(pool, s, a, "demo", now - Duration::days(30 + 20 * i as i64)).await?;
        }
        repo::upsert_link(
            pool,
            &ProviderLinkRow {
                steam_id: s.into(),
                provider: "demo".into(),
                external_id: format!("demo-player-{}", p.n),
                match_evidence: "demo".into(),
                confidence: "high".into(),
                source_url: None,
                linked_at: fmt_ts(now),
            },
        )
        .await?;
        let mut bans = vec![
            BanRecord {
                kind: BanKind::Vac,
                provider: "demo".into(),
                count: Some(p.vac),
                active: p.vac > 0,
                days_since_last: (p.vac + p.game > 0).then_some(p.days_since_ban),
                reason: None,
                project_name: None,
                revoked: None,
                source_url: None,
                observed_at: now,
            },
            BanRecord {
                kind: BanKind::Game,
                provider: "demo".into(),
                count: Some(p.game),
                active: p.game > 0,
                days_since_last: (p.vac + p.game > 0).then_some(p.days_since_ban),
                reason: None,
                project_name: None,
                revoked: None,
                source_url: None,
                observed_at: now,
            },
            BanRecord {
                kind: BanKind::Community,
                provider: "demo".into(),
                count: None,
                active: false,
                days_since_last: None,
                reason: None,
                project_name: None,
                revoked: None,
                source_url: None,
                observed_at: now,
            },
            BanRecord {
                kind: BanKind::Economy,
                provider: "demo".into(),
                count: None,
                active: false,
                days_since_last: None,
                reason: None,
                project_name: None,
                revoked: None,
                source_url: None,
                observed_at: now,
            },
        ];
        if p.n == 7 {
            bans.push(BanRecord {
                kind: BanKind::Project,
                provider: "demo".into(),
                count: None,
                active: true,
                days_since_last: None,
                reason: Some("Вымышленная причина: макросы (демо)".into()),
                project_name: Some("[DEMO] Ночной Рейд".into()),
                revoked: Some(false),
                source_url: None,
                observed_at: now - Duration::days(12),
            });
        }
        repo::replace_bans(pool, s, "demo", &bans).await?;

        // Покрытие игрока = окна приложения в пределах observed_days.
        let obs_from = now - Duration::days(p.observed_days);
        for (a, b) in &app_windows {
            let a2 = (*a).max(obs_from);
            if *b > a2 {
                repo::add_coverage(pool, s, "demo", a2, *b).await?;
            }
        }
    }

    // Сессии: группы играют вместе, сдвиги входа/выхода ±.
    for d in (0..90).rev() {
        let day = (now - Duration::days(d)).duration_trunc(Duration::days(1)).unwrap_or(now);
        let weekend = matches!((day + Duration::hours(3)).format("%u").to_string().as_str(), "5" | "6" | "7");
        for group in 0..5usize {
            let members: Vec<&DemoPlayer> = PLAYERS.iter().filter(|p| p.group == group).collect();
            let lead = members[0];
            let prob = (lead.play_prob + if weekend { 0.15 } else { 0.0 }).min(0.95);
            if rng.f() > prob {
                continue;
            }
            let start = day + Duration::hours(lead.start_hour_utc) + Duration::minutes(rng.range(-60, 90));
            let dur = Duration::minutes(rng.range(70, if weekend { 360 } else { 260 }));
            for m in members {
                if rng.f() > 0.85 {
                    continue;
                }
                let st = start + Duration::minutes(rng.range(-20, 25));
                let en = st + dur + Duration::minutes(rng.range(-40, 40));
                insert_demo_session(pool, m, st, en, now, &app_windows).await?;
            }
        }
    }

    // Трекер и события последних дней
    for n in [1u64, 2, 4, 7] {
        repo::set_tracked(pool, demo_steam_id(n).as_str(), true).await?;
    }
    let s1 = repo::create_squad(pool, "[DEMO] Клан «Северяне»", "#2dd4bf", Some("Вымышленный отряд для демонстрации")).await?;
    for n in [1u64, 2, 3] {
        repo::add_squad_member(pool, s1, demo_steam_id(n).as_str(), "demo").await?;
    }
    let s2 = repo::create_squad(pool, "[DEMO] Дуо «Бетон»", "#a3e635", None).await?;
    for n in [4u64, 5] {
        repo::add_squad_member(pool, s2, demo_steam_id(n).as_str(), "demo").await?;
    }
    for p in PLAYERS.iter().filter(|p| [1, 2, 4, 7].contains(&p.n)) {
        let sid = demo_steam_id(p.n);
        let sessions = repo::list_sessions(pool, sid.as_str(), &fmt_ts(now - Duration::days(3)), &fmt_ts(now)).await?;
        for se in sessions {
            let (Some(sl), Some(su)) = (crate::infrastructure::clock::parse_ts(&se.start_lower), crate::infrastructure::clock::parse_ts(&se.start_upper))
            else {
                continue;
            };
            repo::insert_event(
                pool,
                &NewEvent {
                    steam_id: Some(sid.as_str()),
                    provider: "demo",
                    kind: "joined_observed_server",
                    dedup_key: format!("demo:join:{}:{}", sid, se.start_upper),
                    occurred_lower: sl,
                    occurred_upper: su,
                    approximate: true,
                    server_id: se.server_id.as_deref(),
                    details: serde_json::json!({"poll_interval_secs": POLL_MIN * 60, "demo": true}),
                },
            )
            .await?;
            if let Some(eu) = se.end_upper.as_deref().and_then(crate::infrastructure::clock::parse_ts)
                && let Some(el) = crate::infrastructure::clock::parse_ts(&se.end_lower)
            {
                repo::insert_event(
                    pool,
                    &NewEvent {
                        steam_id: Some(sid.as_str()),
                        provider: "demo",
                        kind: "left_observed_server",
                        dedup_key: format!("demo:left:{}:{}", sid, se.start_upper),
                        occurred_lower: el,
                        occurred_upper: eu,
                        approximate: true,
                        server_id: se.server_id.as_deref(),
                        details: serde_json::json!({"poll_interval_secs": POLL_MIN * 60, "demo": true}),
                    },
                )
                .await?;
            }
        }
    }
    repo::insert_event(
        pool,
        &NewEvent {
            steam_id: Some(demo_steam_id(4).as_str()),
            provider: "demo",
            kind: "name_changed",
            dedup_key: "demo:name:4".into(),
            occurred_lower: now - Duration::days(1) - Duration::minutes(10),
            occurred_upper: now - Duration::days(1),
            approximate: true,
            server_id: None,
            details: serde_json::json!({"from": "DEMO·бетонщик", "to": "DEMO·Бетон", "demo": true}),
        },
    )
    .await?;
    repo::insert_event(
        pool,
        &NewEvent {
            steam_id: None,
            provider: "demo",
            kind: "source_unavailable",
            dedup_key: "demo:outage:1".into(),
            occurred_lower: now - Duration::hours(30),
            occurred_upper: now - Duration::hours(30),
            approximate: false,
            server_id: None,
            details: serde_json::json!({"provider": "demo", "error": "timeout (вымышленный сбой)", "demo": true}),
        },
    )
    .await?;
    repo::record_health_ok(pool, "demo").await?;
    repo::set_setting(pool, "demo_seeded", &fmt_ts(now)).await?;
    Ok(())
}

async fn insert_demo_session(
    pool: &SqlitePool,
    p: &DemoPlayer,
    st: DateTime<Utc>,
    en: DateTime<Utc>,
    now: DateTime<Utc>,
    windows: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Result<(), sqlx::Error> {
    if st >= now || st < now - Duration::days(p.observed_days) {
        return Ok(());
    }
    // Сессия видна только внутри окна наблюдения.
    let Some((wa, wb)) = windows.iter().find(|(a, b)| st >= *a && st < *b) else { return Ok(()) };
    let _ = wa;
    let start_upper = floor5(st) + Duration::minutes(POLL_MIN);
    let start_lower = floor5(st);
    let (end_lower, end_upper, open) = if en >= now {
        (floor5(now), None, true)
    } else if en >= *wb {
        // Наблюдение оборвалось раньше конца сессии — конец неизвестен.
        (floor5(*wb), None, false)
    } else {
        (floor5(en), Some(floor5(en) + Duration::minutes(POLL_MIN)), false)
    };
    if end_lower <= start_upper {
        return Ok(());
    }
    let sid = demo_steam_id(p.n);
    let server = format!("demo:{}", group_server(p.group));
    repo::upsert_session(
        pool,
        &NewSession {
            steam_id: sid.as_str(),
            provider: "demo",
            kind: "inferred_from_polls",
            server_id: Some(&server),
            start_lower,
            start_upper,
            end_lower,
            end_upper,
            is_open: open,
            external_id: None,
        },
    )
    .await
}

/// Симуляция присутствия для трекера в демо-режиме.
pub struct DemoPresence;

#[async_trait]
impl PresenceSource for DemoPresence {
    fn id(&self) -> &'static str {
        "demo"
    }
    fn min_poll_secs(&self) -> u64 {
        30
    }
    async fn presence(&self, ids: &[SteamId64]) -> Result<Vec<Presence>, ProviderError> {
        let now = Utc::now();
        Ok(ids
            .iter()
            .map(|id| {
                let Some(p) = PLAYERS.iter().find(|p| demo_steam_id(p.n) == *id) else {
                    return Presence { steam_id: id.clone(), state: PresenceState::Unknown, server_id: None, server_name: None, persona_name: None };
                };
                // Онлайн в «своё» окно ±3 ч; каждые 20 минут состояние может смениться.
                let h = now.hour() as i64;
                let in_window = (h - p.start_hour_utc).rem_euclid(24) < 4;
                let bucket = now.timestamp() / 1200 + p.n as i64;
                let online = in_window && bucket % 4 != 0 || (!in_window && bucket % 9 == 0);
                Presence {
                    steam_id: id.clone(),
                    state: if online { PresenceState::Online } else { PresenceState::Offline },
                    server_id: online.then(|| format!("demo:{}", group_server(p.group))),
                    server_name: None,
                    persona_name: Some(p.name.to_string()),
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::db::open_memory;

    #[test]
    fn demo_ids_are_valid_and_outside_issued_range() {
        let id = demo_steam_id(1);
        assert!(is_demo_id(&id));
        assert!(SteamId64::parse(id.as_str()).is_ok());
        assert!(!is_demo_id(&SteamId64::parse("76561197960287930").unwrap()));
    }

    #[tokio::test]
    async fn seed_is_idempotent_and_marked_demo() {
        let pool = open_memory().await.unwrap();
        let now = Utc::now();
        seed(&pool, now).await.unwrap();
        let n1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&pool).await.unwrap();
        seed(&pool, now).await.unwrap();
        let n2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&pool).await.unwrap();
        assert_eq!(n1, n2);
        assert!(n1 > 50);
        let non_demo: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM players WHERE is_demo = 0").fetch_one(&pool).await.unwrap();
        assert_eq!(non_demo, 0);
        let names: Vec<String> = sqlx::query_scalar("SELECT persona_name FROM players").fetch_all(&pool).await.unwrap();
        assert!(names.iter().all(|n| n.starts_with("DEMO·")));
        let bm_names: Vec<String> = sqlx::query_scalar("SELECT name FROM bm_players").fetch_all(&pool).await.unwrap();
        assert!(bm_names.len() > 40 && bm_names.iter().all(|n| n.starts_with("DEMO·")));
        let b1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bm_sessions").fetch_one(&pool).await.unwrap();
        seed(&pool, now).await.unwrap();
        let b2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bm_sessions").fetch_one(&pool).await.unwrap();
        assert_eq!(b1, b2);
    }
}
