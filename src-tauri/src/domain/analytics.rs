//! Аналитика активности.
//!
//! Принципы:
//! * «неизвестно» ≠ «офлайн»: знаменатель — только наблюдаемое покрытие;
//! * переходы между опросами (окна неопределённости) исключаются и из числителя, и из знаменателя;
//! * групповой анализ — sweep-line по интервалам, без перемножения «вероятностей»;
//! * локальное время — через правила IANA (chrono-tz), моменты хранятся в UTC.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::BTreeMap;

pub type Ts = DateTime<Utc>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Interval {
    pub start: Ts,
    pub end: Ts,
}

impl Interval {
    pub fn new(start: Ts, end: Ts) -> Option<Self> {
        (end > start).then_some(Self { start, end })
    }
    pub fn secs(&self) -> i64 {
        (self.end - self.start).num_seconds()
    }
}

/// Сортирует и сливает пересекающиеся/смежные интервалы.
pub fn normalize(mut v: Vec<Interval>) -> Vec<Interval> {
    v.retain(|i| i.end > i.start);
    v.sort_by_key(|i| i.start);
    let mut out: Vec<Interval> = Vec::with_capacity(v.len());
    for i in v {
        match out.last_mut() {
            Some(last) if i.start <= last.end => {
                if i.end > last.end {
                    last.end = i.end;
                }
            }
            _ => out.push(i),
        }
    }
    out
}

/// a − b (оба нормализованы).
pub fn subtract(a: &[Interval], b: &[Interval]) -> Vec<Interval> {
    let mut out = Vec::new();
    let mut j = 0;
    for &x in a {
        let mut cur = x.start;
        while j < b.len() && b[j].end <= cur {
            j += 1;
        }
        let mut k = j;
        while k < b.len() && b[k].start < x.end {
            if b[k].start > cur {
                out.push(Interval { start: cur, end: b[k].start });
            }
            if b[k].end > cur {
                cur = b[k].end;
            }
            k += 1;
        }
        if cur < x.end {
            out.push(Interval { start: cur, end: x.end });
        }
    }
    out
}

/// a ∩ b (оба нормализованы).
pub fn intersect(a: &[Interval], b: &[Interval]) -> Vec<Interval> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        let s = a[i].start.max(b[j].start);
        let e = a[i].end.min(b[j].end);
        if e > s {
            out.push(Interval { start: s, end: e });
        }
        if a[i].end < b[j].end {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

pub fn clip(v: &[Interval], range: Interval) -> Vec<Interval> {
    intersect(v, &[range])
}

pub fn total_secs(v: &[Interval]) -> i64 {
    v.iter().map(Interval::secs).sum()
}

/// Сессия с приблизительными границами (для опросов — интервал между сэмплами).
#[derive(Debug, Clone, Serialize)]
pub struct SessionBounds {
    pub start_lower: Ts,
    pub start_upper: Ts,
    /// Последний момент, когда игрок точно наблюдался онлайн.
    pub end_lower: Ts,
    /// Первый момент, когда наблюдался не онлайн; None — сессия открыта/обрыв наблюдений.
    pub end_upper: Option<Ts>,
    pub server_id: Option<String>,
}

/// Временная линия одного игрока в пределах диапазона.
#[derive(Debug, Clone, Default)]
pub struct PlayerTimeline {
    /// Точно наблюдаемые онлайн-интервалы.
    pub online: Vec<Interval>,
    /// Интервалы, где состояние известно (онлайн или офлайн на наблюдаемых источниках).
    pub known: Vec<Interval>,
}

impl PlayerTimeline {
    pub fn build(sessions: &[SessionBounds], coverage: &[Interval], range: Interval) -> Self {
        let mut online = Vec::new();
        let mut ambiguous = Vec::new();
        for s in sessions {
            if let Some(i) = Interval::new(s.start_upper, s.end_lower) {
                online.push(i);
            }
            if let Some(i) = Interval::new(s.start_lower, s.start_upper) {
                ambiguous.push(i);
            }
            if let Some(eu) = s.end_upper
                && let Some(i) = Interval::new(s.end_lower, eu)
            {
                ambiguous.push(i);
            }
        }
        let ambiguous = normalize(ambiguous);
        let online = normalize(online);
        let mut cov = coverage.to_vec();
        cov.extend(online.iter().copied());
        let known = subtract(&normalize(cov), &ambiguous);
        let online = subtract(&online, &ambiguous);
        Self { online: clip(&online, range), known: clip(&known, range) }
    }
}

/// Сетка 15 минут: все действующие смещения часовых поясов кратны 15 минутам,
/// поэтому каждый слот целиком лежит внутри одного локального часа (включая переходы DST).
const SLOT_SECS: i64 = 15 * 60;

fn floor_slot(t: Ts) -> Ts {
    let s = t.timestamp();
    Utc.timestamp_opt(s - s.rem_euclid(SLOT_SECS), 0).single().unwrap_or(t)
}

fn overlap(sorted: &[Interval], idx: &mut usize, s: Ts, e: Ts) -> i64 {
    while *idx < sorted.len() && sorted[*idx].end <= s {
        *idx += 1;
    }
    let mut total = 0;
    let mut j = *idx;
    while j < sorted.len() && sorted[j].start < e {
        let os = sorted[j].start.max(s);
        let oe = sorted[j].end.min(e);
        if oe > os {
            total += (oe - os).num_seconds();
        }
        j += 1;
    }
    total
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CellState {
    /// Достаточно наблюдений.
    Known,
    /// Покрытие есть, но меньше порога.
    Insufficient,
    /// Покрытия нет (нулевой знаменатель).
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeatCell {
    /// 0 = понедельник
    pub weekday: u8,
    pub hour: u8,
    pub online_secs: i64,
    pub known_secs: i64,
    /// Число различных локальных дат с покрытием в этой ячейке.
    pub sample_days: u32,
    /// online / known; None при state != known.
    pub ratio: Option<f64>,
    pub state: CellState,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyPoint {
    pub date: NaiveDate,
    pub online_secs: i64,
    pub known_secs: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekdayPoint {
    pub weekday: u8,
    pub online_secs: i64,
    pub known_secs: i64,
    pub ratio: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityAggregate {
    pub timezone: String,
    pub range_start: Ts,
    pub range_end: Ts,
    pub range_secs: i64,
    pub known_secs: i64,
    pub online_secs: i64,
    /// known / range
    pub coverage: f64,
    pub min_known_secs_per_cell: i64,
    pub cells: Vec<HeatCell>,
    pub daily: Vec<DailyPoint>,
    pub weekdays: Vec<WeekdayPoint>,
    pub typical_offline: TypicalOffline,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TypicalOffline {
    /// Мало данных — прогноз не строится.
    InsufficientData { known_cells: usize, required_cells: usize },
    /// Часы недели (weekday, hour), когда игрок обычно НЕ наблюдался на отслеживаемых источниках.
    Available { hours: Vec<(u8, u8)>, threshold: f64 },
}

pub struct HeatmapOptions {
    pub tz: Tz,
    /// Минимальное покрытие ячейки для статуса known.
    pub min_known_secs: i64,
    /// Минимум ячеек с known для оценки типичных офлайн-часов.
    pub min_known_cells_for_offline: usize,
    /// Ячейка считается «обычно офлайн», если ratio ≤ порога.
    pub offline_ratio_threshold: f64,
    /// Минимум различных дней наблюдения в ячейке, чтобы считать её «типичной».
    pub min_sample_days: u32,
}

impl HeatmapOptions {
    pub fn new(tz: Tz) -> Self {
        Self { tz, min_known_secs: 30 * 60, min_known_cells_for_offline: 84, offline_ratio_threshold: 0.1, min_sample_days: 3 }
    }
}

pub fn aggregate_activity(tl: &PlayerTimeline, range: Interval, opt: &HeatmapOptions) -> ActivityAggregate {
    let mut online = [0i64; 168];
    let mut known = [0i64; 168];
    let mut last_date: [Option<NaiveDate>; 168] = [None; 168];
    let mut days = [0u32; 168];
    let mut daily: BTreeMap<NaiveDate, (i64, i64)> = BTreeMap::new();
    let (mut oi, mut ki) = (0usize, 0usize);
    let mut t = floor_slot(range.start);
    while t < range.end {
        let s = t.max(range.start);
        let e = (t + Duration::seconds(SLOT_SECS)).min(range.end);
        let k = overlap(&tl.known, &mut ki, s, e);
        if k > 0 {
            let o = overlap(&tl.online, &mut oi, s, e).min(k);
            let local = s.with_timezone(&opt.tz);
            let idx = local.weekday().num_days_from_monday() as usize * 24 + local.hour() as usize;
            online[idx] += o;
            known[idx] += k;
            let d = local.date_naive();
            if last_date[idx] != Some(d) {
                last_date[idx] = Some(d);
                days[idx] += 1;
            }
            let ent = daily.entry(d).or_default();
            ent.0 += o;
            ent.1 += k;
        }
        t += Duration::seconds(SLOT_SECS);
    }
    let cells: Vec<HeatCell> = (0..168)
        .map(|i| {
            let state = if known[i] == 0 {
                CellState::Unknown
            } else if known[i] < opt.min_known_secs {
                CellState::Insufficient
            } else {
                CellState::Known
            };
            HeatCell {
                weekday: (i / 24) as u8,
                hour: (i % 24) as u8,
                online_secs: online[i],
                known_secs: known[i],
                sample_days: days[i],
                ratio: (state == CellState::Known).then(|| online[i] as f64 / known[i] as f64),
                state,
            }
        })
        .collect();
    let weekdays = (0..7u8)
        .map(|w| {
            let (o, k) = cells.iter().filter(|c| c.weekday == w).fold((0, 0), |a, c| (a.0 + c.online_secs, a.1 + c.known_secs));
            WeekdayPoint { weekday: w, online_secs: o, known_secs: k, ratio: (k >= opt.min_known_secs).then(|| o as f64 / k as f64) }
        })
        .collect();
    let known_cells = cells.iter().filter(|c| c.state == CellState::Known && c.sample_days >= opt.min_sample_days).count();
    let typical_offline = if known_cells < opt.min_known_cells_for_offline {
        TypicalOffline::InsufficientData { known_cells, required_cells: opt.min_known_cells_for_offline }
    } else {
        TypicalOffline::Available {
            hours: cells
                .iter()
                .filter(|c| c.sample_days >= opt.min_sample_days && c.ratio.is_some_and(|r| r <= opt.offline_ratio_threshold))
                .map(|c| (c.weekday, c.hour))
                .collect(),
            threshold: opt.offline_ratio_threshold,
        }
    };
    let known_secs = total_secs(&tl.known);
    let range_secs = range.secs().max(1);
    ActivityAggregate {
        timezone: opt.tz.name().to_string(),
        range_start: range.start,
        range_end: range.end,
        range_secs,
        known_secs,
        online_secs: total_secs(&tl.online),
        coverage: known_secs as f64 / range_secs as f64,
        min_known_secs_per_cell: opt.min_known_secs,
        cells,
        daily: daily.into_iter().map(|(date, (o, k))| DailyPoint { date, online_secs: o, known_secs: k }).collect(),
        weekdays,
        typical_offline,
    }
}

// ---------------------------------------------------------------------------
// Групповой анализ (sweep-line)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SquadState {
    /// Хотя бы один участник точно наблюдался онлайн.
    SomeOnline,
    /// Все участники наблюдались и ни один не онлайн.
    AllObservedOffline,
    /// Нет надёжного покрытия хотя бы для одного участника, и никто не онлайн.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct SquadSegment {
    pub start: Ts,
    pub end: Ts,
    pub online_mask: u32,
    pub known_mask: u32,
    pub online_count: u32,
    pub state: SquadState,
}

#[derive(Debug, Clone, Serialize)]
pub struct SquadHourCell {
    pub weekday: u8,
    pub hour: u8,
    pub some_online_secs: i64,
    pub all_offline_secs: i64,
    pub unknown_secs: i64,
    /// all_offline / (all_offline + some_online), если известно достаточно.
    pub all_offline_ratio: Option<f64>,
    pub sample_days: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairOverlap {
    pub a: usize,
    pub b: usize,
    pub co_online_secs: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SquadAnalysis {
    pub members: usize,
    pub range_start: Ts,
    pub range_end: Ts,
    pub segments: Vec<SquadSegment>,
    pub hours: Vec<SquadHourCell>,
    pub pairs: Vec<PairOverlap>,
    /// Секунды с ровно k участниками онлайн (k = индекс); учитываются только полностью известные отрезки и отрезки с онлайн.
    pub concurrency_secs: Vec<i64>,
    pub some_online_secs: i64,
    pub all_offline_secs: i64,
    pub unknown_secs: i64,
    pub typical_offline: TypicalOffline,
}

pub fn analyze_squad(members: &[PlayerTimeline], range: Interval, opt: &HeatmapOptions) -> SquadAnalysis {
    let n = members.len().min(32);
    let full: u32 = if n == 32 { u32::MAX } else { (1u32 << n) - 1 };
    // события: (время, участник, онлайн?, +1/-1)
    let mut ev: Vec<(Ts, usize, bool, bool)> = Vec::new();
    for (m, tl) in members.iter().take(n).enumerate() {
        for i in &tl.online {
            ev.push((i.start, m, true, true));
            ev.push((i.end, m, true, false));
        }
        for i in &tl.known {
            ev.push((i.start, m, false, true));
            ev.push((i.end, m, false, false));
        }
    }
    ev.sort_by_key(|e| e.0);
    let mut times: Vec<Ts> = ev.iter().map(|e| e.0).chain([range.start, range.end]).collect();
    times.sort();
    times.dedup();
    let mut online_mask = 0u32;
    let mut known_mask = 0u32;
    let mut segments: Vec<SquadSegment> = Vec::new();
    let mut i = 0;
    for w in times.windows(2) {
        let (t, next) = (w[0], w[1]);
        while i < ev.len() && ev[i].0 <= t {
            let (_, m, is_online, open) = ev[i];
            let bit = 1u32 << m;
            match (is_online, open) {
                (true, true) => online_mask |= bit,
                (true, false) => online_mask &= !bit,
                (false, true) => known_mask |= bit,
                (false, false) => known_mask &= !bit,
            }
            i += 1;
        }
        let s = t.max(range.start);
        let e = next.min(range.end);
        if e <= s {
            continue;
        }
        let effective_known = known_mask | online_mask;
        let state = if online_mask != 0 {
            SquadState::SomeOnline
        } else if effective_known & full == full {
            SquadState::AllObservedOffline
        } else {
            SquadState::Unknown
        };
        match segments.last_mut() {
            Some(last) if last.end == s && last.online_mask == online_mask && last.known_mask == effective_known => last.end = e,
            _ => segments.push(SquadSegment { start: s, end: e, online_mask, known_mask: effective_known, online_count: online_mask.count_ones(), state }),
        }
    }

    // Агрегаты
    let mut hours: Vec<SquadHourCell> = (0..168)
        .map(|k| SquadHourCell {
            weekday: (k / 24) as u8,
            hour: (k % 24) as u8,
            some_online_secs: 0,
            all_offline_secs: 0,
            unknown_secs: 0,
            all_offline_ratio: None,
            sample_days: 0,
        })
        .collect();
    let mut last_day: Vec<Option<NaiveDate>> = vec![None; 168];
    let mut concurrency = vec![0i64; n + 1];
    let mut pair = vec![vec![0i64; n]; n];
    let (mut so, mut ao, mut un) = (0, 0, 0);
    let mut covered_secs: i64 = 0;
    for seg in &segments {
        let len = (seg.end - seg.start).num_seconds();
        covered_secs += len;
        match seg.state {
            SquadState::SomeOnline => so += len,
            SquadState::AllObservedOffline => ao += len,
            SquadState::Unknown => un += len,
        }
        if seg.state != SquadState::Unknown {
            concurrency[seg.online_count as usize] += len;
        }
        for (a, row) in pair.iter_mut().enumerate() {
            if seg.online_mask & (1 << a) == 0 {
                continue;
            }
            for (b, cell) in row.iter_mut().enumerate().skip(a + 1) {
                if seg.online_mask & (1 << b) != 0 {
                    *cell += len;
                }
            }
        }
        // разбивка по часам недели
        let mut t = seg.start;
        while t < seg.end {
            let e = (floor_slot(t) + Duration::seconds(SLOT_SECS)).min(seg.end);
            let l = (e - t).num_seconds();
            let local = t.with_timezone(&opt.tz);
            let idx = local.weekday().num_days_from_monday() as usize * 24 + local.hour() as usize;
            let c = &mut hours[idx];
            if seg.state != SquadState::Unknown && last_day[idx] != Some(local.date_naive()) {
                last_day[idx] = Some(local.date_naive());
                c.sample_days += 1;
            }
            match seg.state {
                SquadState::SomeOnline => c.some_online_secs += l,
                SquadState::AllObservedOffline => c.all_offline_secs += l,
                SquadState::Unknown => c.unknown_secs += l,
            }
            t = e;
        }
    }
    // Хвост диапазона без сегментов — неизвестно.
    un += (range.secs() - covered_secs).max(0);
    for c in hours.iter_mut() {
        let known = c.some_online_secs + c.all_offline_secs;
        if known >= opt.min_known_secs {
            c.all_offline_ratio = Some(c.all_offline_secs as f64 / known as f64);
        }
    }
    let known_cells = hours.iter().filter(|c| c.all_offline_ratio.is_some() && c.sample_days >= opt.min_sample_days).count();
    let typical_offline = if known_cells < opt.min_known_cells_for_offline {
        TypicalOffline::InsufficientData { known_cells, required_cells: opt.min_known_cells_for_offline }
    } else {
        let thr = 1.0 - opt.offline_ratio_threshold;
        TypicalOffline::Available {
            hours: hours
                .iter()
                .filter(|c| c.sample_days >= opt.min_sample_days && c.all_offline_ratio.is_some_and(|r| r >= thr))
                .map(|c| (c.weekday, c.hour))
                .collect(),
            threshold: thr,
        }
    };
    let mut pairs = Vec::new();
    for (a, row) in pair.iter().enumerate() {
        for (b, &v) in row.iter().enumerate().skip(a + 1) {
            pairs.push(PairOverlap { a, b, co_online_secs: v });
        }
    }
    SquadAnalysis {
        members: n,
        range_start: range.start,
        range_end: range.end,
        segments,
        hours,
        pairs,
        concurrency_secs: concurrency,
        some_online_secs: so,
        all_offline_secs: ao,
        unknown_secs: un,
        typical_offline,
    }
}

// ---------------------------------------------------------------------------
// Гипотезы о тиммейтах — только на основе реальных совместных сессий на одном сервере
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CoSessionEvidence {
    pub server_id: String,
    pub start: Ts,
    pub end: Ts,
    pub overlap_secs: i64,
}

/// Совместные сессии на одном и том же сервере с перекрытием ≥ `min_overlap_secs`.
pub fn co_sessions(a: &[SessionBounds], b: &[SessionBounds], min_overlap_secs: i64) -> Vec<CoSessionEvidence> {
    let mut out = Vec::new();
    for x in a {
        let Some(sx) = &x.server_id else { continue };
        for y in b {
            if y.server_id.as_deref() != Some(sx.as_str()) {
                continue;
            }
            let s = x.start_upper.max(y.start_upper);
            let e = x.end_lower.min(y.end_lower);
            if e > s && (e - s).num_seconds() >= min_overlap_secs {
                out.push(CoSessionEvidence { server_id: sx.clone(), start: s, end: e, overlap_secs: (e - s).num_seconds() });
            }
        }
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.start));
    out
}

/// Восстановление сессий из последовательности опросов.
/// Возвращает (сессии, интервалы покрытия). Разрыв между сэмплами > `max_gap` = нет покрытия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollState {
    Online,
    Offline,
    /// Источник не ответил — НЕ офлайн.
    Unknown,
}

pub fn reconstruct_from_polls(samples: &[(Ts, PollState, Option<String>)], max_gap: Duration) -> (Vec<SessionBounds>, Vec<Interval>) {
    let mut sessions: Vec<SessionBounds> = Vec::new();
    let mut coverage = Vec::new();
    let mut prev: Option<(Ts, PollState, Option<String>)> = None;
    let mut open: Option<SessionBounds> = None;
    for (t, st, srv) in samples.iter().cloned() {
        if st == PollState::Unknown {
            // обрыв наблюдения: открытая сессия заканчивается «где-то после» последнего онлайн-сэмпла
            if let Some(s) = open.take() {
                sessions.push(s);
            }
            prev = None;
            continue;
        }
        let contiguous = prev.as_ref().is_some_and(|(pt, _, _)| t - *pt <= max_gap);
        if contiguous && let Some((pt, _, _)) = &prev {
            coverage.push(Interval { start: *pt, end: t });
        }
        match (st, open.as_mut()) {
            (PollState::Online, Some(s)) if contiguous && s.server_id == srv => s.end_lower = t,
            (PollState::Online, _) => {
                if let Some(mut s) = open.take() {
                    if contiguous {
                        s.end_upper = Some(t);
                    }
                    sessions.push(s);
                }
                let start_lower = if contiguous { prev.as_ref().map(|p| p.0).unwrap_or(t) } else { t };
                open = Some(SessionBounds { start_lower, start_upper: t, end_lower: t, end_upper: None, server_id: srv.clone() });
            }
            (PollState::Offline, _) => {
                if let Some(mut s) = open.take() {
                    if contiguous {
                        s.end_upper = Some(t);
                    }
                    sessions.push(s);
                }
            }
            (PollState::Unknown, _) => unreachable!(),
        }
        prev = Some((t, st, srv));
    }
    if let Some(s) = open.take() {
        sessions.push(s);
    }
    (sessions, normalize(coverage))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Ts {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }
    fn iv(a: &str, b: &str) -> Interval {
        Interval::new(t(a), t(b)).unwrap()
    }
    fn sess(a: &str, b: &str) -> SessionBounds {
        SessionBounds { start_lower: t(a), start_upper: t(a), end_lower: t(b), end_upper: Some(t(b)), server_id: Some("s1".into()) }
    }

    #[test]
    fn interval_algebra() {
        let a = normalize(vec![iv("2026-01-01T00:00:00Z", "2026-01-01T02:00:00Z"), iv("2026-01-01T01:00:00Z", "2026-01-01T03:00:00Z")]);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].secs(), 3 * 3600);
        let b = vec![iv("2026-01-01T01:00:00Z", "2026-01-01T01:30:00Z")];
        let d = subtract(&a, &b);
        assert_eq!(total_secs(&d), 3 * 3600 - 1800);
        assert_eq!(total_secs(&intersect(&a, &b)), 1800);
    }

    #[test]
    fn missing_hours_are_unknown_not_offline() {
        // покрытие только с 10:00 до 12:00 UTC, онлайн 10:00–11:00
        let range = iv("2026-01-05T00:00:00Z", "2026-01-06T00:00:00Z"); // понедельник
        let tl = PlayerTimeline::build(&[sess("2026-01-05T10:00:00Z", "2026-01-05T11:00:00Z")], &[iv("2026-01-05T10:00:00Z", "2026-01-05T12:00:00Z")], range);
        let agg = aggregate_activity(&tl, range, &HeatmapOptions::new(chrono_tz::UTC));
        let c10 = &agg.cells[10];
        assert_eq!(c10.state, CellState::Known);
        assert_eq!(c10.ratio, Some(1.0));
        let c11 = &agg.cells[11];
        assert_eq!(c11.ratio, Some(0.0));
        // все остальные часы — unknown, а не 0%
        assert_eq!(agg.cells[3].state, CellState::Unknown);
        assert_eq!(agg.cells[3].ratio, None);
        assert!((agg.coverage - 2.0 / 24.0).abs() < 1e-9);
        assert!(matches!(agg.typical_offline, TypicalOffline::InsufficientData { .. }));
    }

    #[test]
    fn ambiguous_poll_edges_excluded() {
        // сэмплы каждые 10 минут: off 10:00, on 10:10, on 10:20, off 10:30
        let s = [
            (t("2026-01-05T10:00:00Z"), PollState::Offline, None),
            (t("2026-01-05T10:10:00Z"), PollState::Online, None),
            (t("2026-01-05T10:20:00Z"), PollState::Online, None),
            (t("2026-01-05T10:30:00Z"), PollState::Offline, None),
        ];
        let (sessions, cov) = reconstruct_from_polls(&s, Duration::minutes(25));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].start_lower, t("2026-01-05T10:00:00Z"));
        assert_eq!(sessions[0].start_upper, t("2026-01-05T10:10:00Z"));
        assert_eq!(sessions[0].end_lower, t("2026-01-05T10:20:00Z"));
        assert_eq!(sessions[0].end_upper, Some(t("2026-01-05T10:30:00Z")));
        let range = iv("2026-01-05T10:00:00Z", "2026-01-05T11:00:00Z");
        let tl = PlayerTimeline::build(&sessions, &cov, range);
        assert_eq!(total_secs(&tl.online), 600); // только точно наблюдаемые 10 мин
        assert_eq!(total_secs(&tl.known), 600); // переходы исключены из знаменателя
    }

    #[test]
    fn source_outage_is_not_logout() {
        let s = [
            (t("2026-01-05T10:00:00Z"), PollState::Online, None),
            (t("2026-01-05T10:10:00Z"), PollState::Unknown, None),
            (t("2026-01-05T10:20:00Z"), PollState::Offline, None),
        ];
        let (sessions, cov) = reconstruct_from_polls(&s, Duration::minutes(25));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].end_upper, None, "конец сессии неизвестен");
        assert!(cov.is_empty());
    }

    #[test]
    fn long_gap_breaks_coverage() {
        let s = [(t("2026-01-05T10:00:00Z"), PollState::Offline, None), (t("2026-01-05T14:00:00Z"), PollState::Offline, None)];
        let (_, cov) = reconstruct_from_polls(&s, Duration::minutes(25));
        assert!(cov.is_empty());
    }

    #[test]
    fn dst_transitions_berlin_spring_and_autumn() {
        // Берлин: 29.03.2026 02:00 → 03:00 (переход на летнее время). 01:00Z = 03:00 CEST.
        let tz: Tz = "Europe/Berlin".parse().unwrap();
        let range = iv("2026-03-29T00:00:00Z", "2026-03-29T02:00:00Z");
        let tl = PlayerTimeline { online: vec![range], known: vec![range] };
        let agg = aggregate_activity(&tl, range, &HeatmapOptions::new(tz));
        // воскресенье = 6; 00:00Z = 01:00 CET, 01:00Z = 03:00 CEST — часа 02 не существует
        assert_eq!(agg.cells[6 * 24 + 1].known_secs, 3600);
        assert_eq!(agg.cells[6 * 24 + 2].known_secs, 0);
        assert_eq!(agg.cells[6 * 24 + 3].known_secs, 3600);
        // Осенний переход: 25.10.2026 03:00 CEST → 02:00 CET; час 02 встречается дважды
        let range = iv("2026-10-25T00:00:00Z", "2026-10-25T01:00:00Z");
        let tl = PlayerTimeline { online: vec![], known: vec![range] };
        let agg = aggregate_activity(&tl, range, &HeatmapOptions::new(tz));
        assert_eq!(agg.cells[6 * 24 + 2].known_secs, 3600);
        assert_eq!(agg.cells[6 * 24 + 2].sample_days, 1);
    }

    #[test]
    fn squad_all_offline_requires_full_coverage() {
        let range = iv("2026-01-05T00:00:00Z", "2026-01-05T04:00:00Z");
        // A известен 00–04, онлайн 01–02. B известен только 00–02, онлайн 01:30–02.
        let a = PlayerTimeline { online: vec![iv("2026-01-05T01:00:00Z", "2026-01-05T02:00:00Z")], known: vec![range] };
        let b = PlayerTimeline {
            online: vec![iv("2026-01-05T01:30:00Z", "2026-01-05T02:00:00Z")],
            known: vec![iv("2026-01-05T00:00:00Z", "2026-01-05T02:00:00Z")],
        };
        let r = analyze_squad(&[a, b], range, &HeatmapOptions::new(chrono_tz::UTC));
        assert_eq!(r.all_offline_secs, 3600); // 00–01: оба известны и офлайн
        assert_eq!(r.some_online_secs, 3600); // 01–02
        assert_eq!(r.unknown_secs, 7200); // 02–04: B неизвестен → не «все офлайн»
        assert_eq!(r.pairs[0].co_online_secs, 1800);
        assert_eq!(r.concurrency_secs[2], 1800);
        assert_eq!(r.concurrency_secs[1], 1800);
        assert_eq!(r.concurrency_secs[0], 3600);
    }

    #[test]
    fn squad_overlapping_sessions_sweep() {
        let range = iv("2026-01-05T00:00:00Z", "2026-01-05T03:00:00Z");
        let mk = |a, b| PlayerTimeline { online: vec![iv(a, b)], known: vec![range] };
        let r = analyze_squad(
            &[
                mk("2026-01-05T00:00:00Z", "2026-01-05T02:00:00Z"),
                mk("2026-01-05T01:00:00Z", "2026-01-05T03:00:00Z"),
                mk("2026-01-05T00:30:00Z", "2026-01-05T01:30:00Z"),
            ],
            range,
            &HeatmapOptions::new(chrono_tz::UTC),
        );
        assert_eq!(r.all_offline_secs, 0);
        assert_eq!(r.unknown_secs, 0);
        assert_eq!(r.concurrency_secs[3], 1800); // 01:00–01:30
        let ab = r.pairs.iter().find(|p| p.a == 0 && p.b == 1).unwrap();
        assert_eq!(ab.co_online_secs, 3600);
    }

    #[test]
    fn co_sessions_require_same_server() {
        let a = vec![sess("2026-01-05T00:00:00Z", "2026-01-05T02:00:00Z")];
        let mut b = vec![sess("2026-01-05T01:00:00Z", "2026-01-05T03:00:00Z")];
        assert_eq!(co_sessions(&a, &b, 1800).len(), 1);
        b[0].server_id = Some("other".into());
        assert!(co_sessions(&a, &b, 1800).is_empty());
    }
}
