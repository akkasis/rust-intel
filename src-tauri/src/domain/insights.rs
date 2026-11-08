//! Выводы по наблюдаемым сессиям: распорядок и прогноз, активность после вайпа, похожесть расписаний.
//! Всё — статистика по наблюдениям, а не утверждения о человеке.

use super::analytics::{ActivityAggregate, CellState};
use super::teammates::ObservedSession;
use chrono::{DateTime, Datelike, Duration, DurationRound, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;

pub type Ts = DateTime<Utc>;

#[derive(Debug, Clone, Serialize)]
pub struct HourForecast {
    pub at: Ts,
    /// Доля онлайн в эту ячейку недели (None — мало данных).
    pub p: Option<f64>,
    pub sample_days: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Routine {
    pub sessions: usize,
    pub sessions_with_known_start: usize,
    /// Часы (локальные) начала сессий: гистограмма 24.
    pub start_hours: Vec<u32>,
    /// Самое частое окно входа (час начала, 2-часовое окно), если данных достаточно.
    pub usual_start: Option<(u8, u8)>,
    pub median_session_secs: Option<i64>,
    /// Средняя игра в день по дням, когда игрок вообще заходил.
    pub avg_daily_secs_active_days: Option<i64>,
    pub active_days: usize,
    pub weekday_avg_secs: Option<i64>,
    pub weekend_avg_secs: Option<i64>,
    /// Прогноз на ближайшие 24 часа по часам.
    pub forecast: Vec<HourForecast>,
    /// Ближайший час с вероятностью ≥ 0.5.
    pub next_likely_online: Option<Ts>,
    pub summary: Vec<String>,
}

fn median(mut v: Vec<i64>) -> Option<i64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

pub fn routine(sessions: &[ObservedSession], agg: &ActivityAggregate, tz: Tz, now: Ts) -> Routine {
    let mut start_hours = vec![0u32; 24];
    let mut durations = vec![];
    let mut per_day: std::collections::BTreeMap<chrono::NaiveDate, i64> = Default::default();
    for s in sessions {
        let end = s.end.unwrap_or(now);
        let secs = (end - s.start).num_seconds().max(0);
        if s.start_known {
            start_hours[s.start.with_timezone(&tz).hour() as usize] += 1;
            if s.end_known || s.end.is_none() {
                durations.push(secs);
            }
        }
        *per_day.entry(s.start.with_timezone(&tz).date_naive()).or_default() += secs;
    }
    let known_starts: usize = start_hours.iter().map(|x| *x as usize).sum();
    let usual_start = (known_starts >= 5)
        .then(|| {
            (0..24u8)
                .map(|h| (h, start_hours[h as usize] + start_hours[((h + 1) % 24) as usize]))
                .max_by_key(|x| x.1)
                .filter(|x| x.1 as f64 / known_starts as f64 >= 0.3)
                .map(|(h, _)| (h, (h + 2) % 24))
        })
        .flatten();
    let (mut wd, mut we) = (vec![], vec![]);
    for (d, secs) in &per_day {
        if d.weekday().number_from_monday() >= 6 { we.push(*secs) } else { wd.push(*secs) }
    }
    let avg = |v: &Vec<i64>| (!v.is_empty()).then(|| v.iter().sum::<i64>() / v.len() as i64);
    // прогноз по ячейкам недели
    let start = now.duration_trunc(Duration::hours(1)).unwrap_or(now) + Duration::hours(1);
    let forecast: Vec<HourForecast> = (0..24)
        .map(|i| {
            let at = start + Duration::hours(i);
            let l = at.with_timezone(&tz);
            let idx = l.weekday().num_days_from_monday() as usize * 24 + l.hour() as usize;
            let c = &agg.cells[idx];
            HourForecast { at, p: (c.state == CellState::Known && c.sample_days >= 3).then_some(c.ratio).flatten(), sample_days: c.sample_days }
        })
        .collect();
    let next_likely_online = forecast.iter().find(|f| f.p.is_some_and(|p| p >= 0.5)).map(|f| f.at);
    let mut summary = vec![];
    if let Some((a, b)) = usual_start {
        summary.push(format!("Обычно заходит между {a:02}:00 и {b:02}:00 ({})", tz.name()));
    }
    if let Some(m) = median(durations.clone()) {
        summary.push(format!("Типичная сессия ≈ {} ч {} мин", m / 3600, (m % 3600) / 60));
    }
    if let (Some(a), Some(b)) = (avg(&wd), avg(&we))
        && a > 0
    {
        let r = b as f64 / a as f64;
        if r >= 1.3 {
            summary.push(format!("По выходным играет дольше (×{r:.1})"));
        } else if r <= 0.75 {
            summary.push(format!("По выходным играет меньше (×{r:.1})"));
        }
    }
    match next_likely_online {
        Some(t) => summary.push(format!("Вероятно будет онлайн с {:02}:00", t.with_timezone(&tz).hour())),
        None if forecast.iter().any(|f| f.p.is_some()) => summary.push("В ближайшие 24 часа заметной вероятности онлайна нет".into()),
        None => summary.push("Для прогноза мало данных (нужно ≥ 3 дней наблюдения в каждом часе)".into()),
    }
    Routine {
        sessions: sessions.len(),
        sessions_with_known_start: known_starts,
        start_hours,
        usual_start,
        median_session_secs: median(durations),
        active_days: per_day.len(),
        avg_daily_secs_active_days: avg(&per_day.values().copied().collect()),
        weekday_avg_secs: avg(&wd),
        weekend_avg_secs: avg(&we),
        forecast,
        next_likely_online,
        summary,
    }
}

// ------------------------------------------------------------------ после вайпа

#[derive(Debug, Clone, Serialize)]
pub struct WipeCycle {
    pub server_id: String,
    pub wipe_at: Ts,
    /// Задержка первого входа после вайпа (None — не заходил в течение 7 дней).
    pub first_login_delay_secs: Option<i64>,
    /// Секунды онлайн по дням 0..6 после вайпа.
    pub daily_secs: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostWipe {
    pub cycles: Vec<WipeCycle>,
    /// Средняя игра по дням 0..6 после вайпа (по циклам, где игрок вообще играл).
    pub avg_daily_secs: Vec<i64>,
    pub median_first_login_delay_secs: Option<i64>,
    /// Доля циклов, где зашёл в первые 2 часа после вайпа.
    pub joins_on_wipe_rate: Option<f64>,
    /// Сервер, на котором больше всего играл, по неделям (для смены серверов).
    pub main_server_by_week: Vec<(chrono::NaiveDate, String, i64)>,
    pub summary: Vec<String>,
}

pub fn post_wipe(sessions: &[ObservedSession], wipes: &[(String, Ts)], tz: Tz, now: Ts) -> PostWipe {
    let mut cycles = vec![];
    for (server, w) in wipes {
        let mine: Vec<&ObservedSession> = sessions.iter().filter(|s| &s.server_id == server).collect();
        let horizon = *w + Duration::days(7);
        if *w > now {
            continue;
        }
        let first = mine.iter().filter(|s| s.start >= *w && s.start < horizon).map(|s| s.start).min();
        let mut daily = vec![0i64; 7];
        for s in &mine {
            let e = s.end.unwrap_or(now);
            for (d, slot) in daily.iter_mut().enumerate() {
                let a = (*w + Duration::days(d as i64)).max(s.start);
                let b = (*w + Duration::days(d as i64 + 1)).min(e);
                if b > a {
                    *slot += (b - a).num_seconds();
                }
            }
        }
        if mine.iter().any(|s| s.start < horizon && s.end.unwrap_or(now) > *w - Duration::days(14)) {
            cycles.push(WipeCycle { server_id: server.clone(), wipe_at: *w, first_login_delay_secs: first.map(|f| (f - *w).num_seconds()), daily_secs: daily });
        }
    }
    cycles.sort_by_key(|c| std::cmp::Reverse(c.wipe_at));
    let played: Vec<&WipeCycle> = cycles.iter().filter(|c| c.daily_secs.iter().sum::<i64>() > 0).collect();
    let avg_daily_secs =
        (0..7).map(|d| if played.is_empty() { 0 } else { played.iter().map(|c| c.daily_secs[d]).sum::<i64>() / played.len() as i64 }).collect::<Vec<_>>();
    let delays: Vec<i64> = cycles.iter().filter_map(|c| c.first_login_delay_secs).collect();
    let joins_on_wipe_rate = (!cycles.is_empty()).then(|| delays.iter().filter(|d| **d <= 7200).count() as f64 / cycles.len() as f64);
    // основной сервер по неделям
    let mut weeks: std::collections::BTreeMap<chrono::NaiveDate, std::collections::HashMap<String, i64>> = Default::default();
    for s in sessions {
        let d = s.start.with_timezone(&tz).date_naive();
        let monday = d - chrono::Days::new(d.weekday().num_days_from_monday() as u64);
        *weeks.entry(monday).or_default().entry(s.server_id.clone()).or_default() += (s.end.unwrap_or(now) - s.start).num_seconds().max(0);
    }
    let main_server_by_week: Vec<(chrono::NaiveDate, String, i64)> =
        weeks.into_iter().filter_map(|(w, m)| m.into_iter().max_by_key(|x| x.1).map(|(s, secs)| (w, s, secs))).collect();
    let median_delay = median(delays);
    let mut summary = vec![];
    if let Some(r) = joins_on_wipe_rate
        && cycles.len() >= 2
    {
        summary.push(format!("Заходит в первые 2 часа после вайпа в {:.0}% вайпов", r * 100.0));
    }
    if let Some(d) = median_delay {
        summary.push(format!("Обычно появляется через {} ч {} мин после вайпа", d / 3600, (d % 3600) / 60));
    }
    if played.len() >= 2 {
        let first2: i64 = avg_daily_secs[..2].iter().sum();
        let rest: i64 = avg_daily_secs[2..].iter().sum::<i64>() / 5;
        if first2 / 2 > rest * 2 {
            summary.push("Основная активность — первые 1–2 дня после вайпа, потом резко меньше".into());
        } else {
            summary.push("Играет равномерно в течение вайпа".into());
        }
    }
    let switches = main_server_by_week.windows(2).filter(|w| w[0].1 != w[1].1).count();
    if switches > 0 {
        summary.push(format!("Менял основной сервер {switches} раз(а) за период"));
    }
    if cycles.is_empty() {
        summary.push("Нет данных о вайпах серверов, где играет игрок".into());
    }
    PostWipe { cycles, avg_daily_secs, median_first_login_delay_secs: median_delay, joins_on_wipe_rate, main_server_by_week, summary }
}

// ------------------------------------------------------------------ похожесть расписаний

/// Профиль 168 часов недели: доля времени онлайн по каждому часу (нормированный).
pub fn week_profile(sessions: &[ObservedSession], tz: Tz, now: Ts) -> Vec<f64> {
    let mut v = vec![0f64; 168];
    for s in sessions {
        let mut t = s.start;
        let end = s.end.unwrap_or(now);
        while t < end {
            let next = (t.duration_trunc(Duration::hours(1)).unwrap_or(t) + Duration::hours(1)).min(end);
            let l = t.with_timezone(&tz);
            v[l.weekday().num_days_from_monday() as usize * 24 + l.hour() as usize] += (next - t).num_seconds() as f64;
            t = next;
        }
    }
    v
}

pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::analytics::{HeatmapOptions, Interval, PlayerTimeline, aggregate_activity};

    fn t(s: &str) -> Ts {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }
    fn sess(server: &str, start: Ts, mins: i64) -> ObservedSession {
        ObservedSession { player: "p".into(), server_id: server.into(), start, start_known: true, end: Some(start + Duration::minutes(mins)), end_known: true }
    }

    #[test]
    fn routine_finds_usual_start_and_forecast() {
        let now = t("2026-11-09T12:00:00Z");
        // 21 день подряд заходит в 18:00 UTC на 3 часа
        let sessions: Vec<_> = (1..=21).map(|d| sess("s", t("2026-10-19T18:00:00Z") + Duration::days(d - 1), 180)).collect();
        let range = Interval::new(now - Duration::days(30), now).unwrap();
        let bounds: Vec<_> = sessions
            .iter()
            .map(|s| crate::domain::analytics::SessionBounds {
                start_lower: s.start,
                start_upper: s.start,
                end_lower: s.end.unwrap(),
                end_upper: s.end,
                server_id: None,
            })
            .collect();
        let cov = vec![Interval::new(t("2026-10-19T00:00:00Z"), now).unwrap()];
        let agg = aggregate_activity(&PlayerTimeline::build(&bounds, &cov, range), range, &HeatmapOptions::new(chrono_tz::UTC));
        let r = routine(&sessions, &agg, chrono_tz::UTC, now);
        assert!(matches!(r.usual_start, Some((17, 19)) | Some((18, 20))), "{:?}", r.usual_start);
        assert_eq!(r.median_session_secs, Some(3 * 3600));
        assert_eq!(r.next_likely_online.map(|x| x.hour()), Some(18));
    }

    #[test]
    fn routine_without_data_does_not_guess() {
        let now = t("2026-11-09T12:00:00Z");
        let range = Interval::new(now - Duration::days(7), now).unwrap();
        let agg = aggregate_activity(&PlayerTimeline::default(), range, &HeatmapOptions::new(chrono_tz::UTC));
        let r = routine(&[], &agg, chrono_tz::UTC, now);
        assert!(r.next_likely_online.is_none() && r.usual_start.is_none());
        assert!(r.forecast.iter().all(|f| f.p.is_none()));
    }

    #[test]
    fn post_wipe_curve_and_delay() {
        let now = t("2026-11-09T12:00:00Z");
        let w1 = t("2026-10-23T18:00:00Z");
        let w2 = t("2026-10-30T18:00:00Z");
        let sessions = vec![sess("s", w1 + Duration::minutes(30), 300), sess("s", w1 + Duration::days(3), 60), sess("s", w2 + Duration::minutes(45), 360)];
        let p = post_wipe(&sessions, &[("s".into(), w1), ("s".into(), w2)], chrono_tz::UTC, now);
        assert_eq!(p.cycles.len(), 2);
        assert_eq!(p.joins_on_wipe_rate, Some(1.0));
        assert!(p.avg_daily_secs[0] > p.avg_daily_secs[3]);
        assert!(p.median_first_login_delay_secs.unwrap() <= 45 * 60);
    }

    #[test]
    fn cosine_similarity_of_schedules() {
        let now = t("2026-11-09T12:00:00Z");
        let a: Vec<_> = (0..10).map(|d| sess("s", t("2026-10-20T18:00:00Z") + Duration::days(d), 120)).collect();
        let b: Vec<_> = (0..10).map(|d| sess("s", t("2026-10-20T18:30:00Z") + Duration::days(d), 120)).collect();
        let c: Vec<_> = (0..10).map(|d| sess("s", t("2026-10-20T06:00:00Z") + Duration::days(d), 120)).collect();
        let pa = week_profile(&a, chrono_tz::UTC, now);
        assert!(cosine(&pa, &week_profile(&b, chrono_tz::UTC, now)) > 0.8);
        assert!(cosine(&pa, &week_profile(&c, chrono_tz::UTC, now)) < 0.1);
    }
}
