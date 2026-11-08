//! Поиск возможных тиммейтов по наблюдаемым сессиям на серверах.
//!
//! Сигналы (по убыванию силы):
//! * совместный вход — оба вошли на тот же сервер с разницей ≤ окна (по умолчанию 10 мин), при этом оба
//!   входа действительно наблюдались (start_known);
//! * совместный выход — аналогично для выхода;
//! * совместное пребывание — пересечение сессий ≥ min_overlap.
//!
//! Просто «были на одном большом сервере» — слабый сигнал, поэтому он весит меньше и сам по себе
//! кандидата не создаёт. Результат — гипотеза с доказательствами, а не утверждение.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

pub type Ts = DateTime<Utc>;

#[derive(Debug, Clone)]
pub struct ObservedSession {
    pub player: String,
    pub server_id: String,
    pub start: Ts,
    pub start_known: bool,
    /// None — сессия идёт сейчас.
    pub end: Option<Ts>,
    pub end_known: bool,
}

impl ObservedSession {
    fn end_or(&self, now: Ts) -> Ts {
        self.end.unwrap_or(now)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TogetherEvidence {
    pub server_id: String,
    pub start: Ts,
    pub end: Ts,
    pub overlap_secs: i64,
    pub joined_together: bool,
    pub left_together: bool,
    /// Разница во времени входа, сек (если оба входа наблюдались).
    pub join_delta_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeammateCandidate {
    pub player: String,
    pub co_sessions: u32,
    pub joined_together: u32,
    pub left_together: u32,
    pub overlap_secs: i64,
    pub servers: Vec<String>,
    pub last_together: Ts,
    pub score: f64,
    /// Доля сессий цели (на общих серверах), где совпал вход или выход.
    pub sync_rate: f64,
    /// high | medium | low
    pub confidence: &'static str,
    pub evidence: Vec<TogetherEvidence>,
}

#[derive(Debug, Clone, Copy)]
pub struct TeammateOptions {
    pub window_secs: i64,
    pub min_overlap_secs: i64,
    pub max_results: usize,
}

impl Default for TeammateOptions {
    fn default() -> Self {
        Self { window_secs: 600, min_overlap_secs: 20 * 60, max_results: 50 }
    }
}

pub fn find_teammates(target: &[ObservedSession], others: &[ObservedSession], now: Ts, opt: TeammateOptions) -> Vec<TeammateCandidate> {
    // индекс других сессий по серверу, отсортированных по началу
    let mut by_server: HashMap<&str, Vec<&ObservedSession>> = HashMap::new();
    let target_player = target.first().map(|t| t.player.as_str());
    for o in others {
        if Some(o.player.as_str()) == target_player {
            continue;
        }
        by_server.entry(o.server_id.as_str()).or_default().push(o);
    }
    for v in by_server.values_mut() {
        v.sort_by_key(|s| s.start);
    }
    let mut acc: HashMap<&str, TeammateCandidate> = HashMap::new();
    for t in target {
        let Some(list) = by_server.get(t.server_id.as_str()) else { continue };
        let t_end = t.end_or(now);
        // все сессии, начавшиеся до конца целевой
        let upto = list.partition_point(|s| s.start < t_end);
        for o in &list[..upto] {
            let o_end = o.end_or(now);
            if o_end <= t.start {
                continue;
            }
            let s = t.start.max(o.start);
            let e = t_end.min(o_end);
            let overlap = (e - s).num_seconds().max(0);
            let join_delta = (t.start_known && o.start_known).then(|| (t.start - o.start).num_seconds().abs());
            let joined = join_delta.is_some_and(|d| d <= opt.window_secs);
            let left = t.end_known && o.end_known && t.end.zip(o.end).is_some_and(|(a, b)| (a - b).num_seconds().abs() <= opt.window_secs);
            let co = overlap >= opt.min_overlap_secs;
            if !(joined || left || co) {
                continue;
            }
            let c = acc.entry(o.player.as_str()).or_insert_with(|| TeammateCandidate {
                player: o.player.clone(),
                co_sessions: 0,
                joined_together: 0,
                left_together: 0,
                overlap_secs: 0,
                servers: vec![],
                last_together: s,
                score: 0.0,
                sync_rate: 0.0,
                confidence: "low",
                evidence: vec![],
            });
            if co {
                c.co_sessions += 1;
            }
            c.joined_together += joined as u32;
            c.left_together += left as u32;
            c.overlap_secs += overlap;
            if !c.servers.contains(&t.server_id) {
                c.servers.push(t.server_id.clone());
            }
            if e > c.last_together {
                c.last_together = e;
            }
            c.evidence.push(TogetherEvidence {
                server_id: t.server_id.clone(),
                start: s,
                end: e,
                overlap_secs: overlap,
                joined_together: joined,
                left_together: left,
                join_delta_secs: join_delta,
            });
        }
    }
    let mut per_server: HashMap<&str, u32> = HashMap::new();
    for t in target {
        *per_server.entry(t.server_id.as_str()).or_default() += 1;
    }
    let mut out: Vec<TeammateCandidate> = acc
        .into_values()
        .filter_map(|mut c| {
            let sync = c.joined_together + c.left_together;
            // Один случайный совпавший вечер — не сигнал. Нужна повторяемость в разных сессиях.
            let sync_sessions = c.evidence.iter().filter(|e| e.joined_together || e.left_together).count();
            if sync_sessions < 2 && c.co_sessions < 5 {
                return None;
            }
            let base: u32 = c.servers.iter().map(|s| per_server.get(s.as_str()).copied().unwrap_or(0)).sum::<u32>().max(1);
            c.sync_rate = sync_sessions as f64 / base as f64;
            // Счёт учитывает и повторяемость, и долю: на людном сервере случайные совпадения неизбежны.
            c.score = (3.0 * c.joined_together as f64 + 3.0 * c.left_together as f64 + c.co_sessions as f64) * (0.25 + c.sync_rate);
            c.confidence = if c.sync_rate >= 0.3 && c.joined_together >= 3 && sync >= 6 {
                "high"
            } else if c.sync_rate >= 0.12 && sync >= 3 {
                "medium"
            } else {
                "low"
            };
            c.evidence.sort_by_key(|e| std::cmp::Reverse(e.start));
            c.evidence.truncate(8);
            Some(c)
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(b.last_together.cmp(&a.last_together)));
    out.truncate(opt.max_results);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn t0() -> Ts {
        DateTime::parse_from_rfc3339("2026-11-01T18:00:00Z").unwrap().with_timezone(&Utc)
    }
    fn s(p: &str, srv: &str, start_min: i64, end_min: i64) -> ObservedSession {
        ObservedSession {
            player: p.into(),
            server_id: srv.into(),
            start: t0() + Duration::minutes(start_min),
            start_known: true,
            end: Some(t0() + Duration::minutes(end_min)),
            end_known: true,
        }
    }

    #[test]
    fn joins_and_leaves_together_rank_first() {
        let mut target = vec![];
        let mut others = vec![];
        for d in 0..4 {
            let base = d * 1440;
            target.push(s("T", "srv", base, base + 180));
            others.push(s("MATE", "srv", base + 3, base + 182)); // вместе вошли и вышли
            others.push(s("RANDOM", "srv", base + 60, base + 300)); // просто пересекается
        }
        let r = find_teammates(&target, &others, t0() + Duration::days(10), TeammateOptions::default());
        assert_eq!(r[0].player, "MATE");
        assert_eq!(r[0].joined_together, 4);
        assert_eq!(r[0].left_together, 4);
        assert_eq!(r[0].confidence, "high");
        let rnd = r.iter().find(|c| c.player == "RANDOM");
        assert!(rnd.is_none_or(|c| c.joined_together == 0 && c.score < r[0].score));
    }

    #[test]
    fn single_coincidence_is_not_a_teammate() {
        let target = vec![s("T", "srv", 0, 120)];
        let others = vec![s("X", "srv", 2, 130)];
        assert!(find_teammates(&target, &others, t0() + Duration::days(1), TeammateOptions::default()).is_empty());
    }

    #[test]
    fn different_server_never_matches() {
        let target: Vec<_> = (0..5).map(|d| s("T", "a", d * 1440, d * 1440 + 120)).collect();
        let others: Vec<_> = (0..5).map(|d| s("X", "b", d * 1440, d * 1440 + 120)).collect();
        assert!(find_teammates(&target, &others, t0() + Duration::days(10), TeammateOptions::default()).is_empty());
    }

    #[test]
    fn unknown_start_does_not_count_as_joined_together() {
        let mut target = vec![];
        let mut others = vec![];
        for d in 0..3 {
            let base = d * 1440;
            target.push(s("T", "srv", base, base + 60));
            let mut o = s("X", "srv", base, base + 200);
            o.start_known = false; // наблюдение началось, когда он уже был онлайн
            o.end_known = false;
            others.push(o);
        }
        let r = find_teammates(&target, &others, t0() + Duration::days(5), TeammateOptions::default());
        assert!(r.is_empty(), "без наблюдаемых входов/выходов и < 5 совместных сессий — не кандидат");
    }
}
