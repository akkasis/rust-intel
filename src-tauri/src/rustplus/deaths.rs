//! Определение смертей тиммейтов по снимкам состава команды Rust+.

use super::grid::grid_pos;
use super::proto::Member;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct MemberSnap {
    pub name: String,
    pub alive: bool,
    pub death_time: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Death {
    pub steam_id: u64,
    pub name: String,
    pub death_time: u32,
    pub x: f32,
    pub y: f32,
    pub grid: Option<String>,
}

pub fn snapshot(members: &[Member]) -> HashMap<u64, MemberSnap> {
    members
        .iter()
        .filter_map(|m| {
            Some((
                m.steam_id?,
                MemberSnap {
                    name: m.name.clone().unwrap_or_default(),
                    alive: m.is_alive.unwrap_or(true),
                    death_time: m.death_time.unwrap_or(0),
                    x: m.x.unwrap_or(0.0),
                    y: m.y.unwrap_or(0.0),
                },
            ))
        })
        .collect()
}

/// Новые смерти между двумя снимками. Первый снимок (prev = None) смертей не порождает,
/// чтобы не спамить при подключении о давно умерших.
pub fn detect(prev: Option<&HashMap<u64, MemberSnap>>, cur: &HashMap<u64, MemberSnap>, map_size: Option<u32>) -> Vec<Death> {
    let Some(prev) = prev else { return vec![] };
    let mut out = vec![];
    for (id, c) in cur {
        let Some(p) = prev.get(id) else { continue };
        let died = !c.alive && (p.alive || (c.death_time != 0 && c.death_time != p.death_time));
        if died {
            out.push(Death {
                steam_id: *id,
                name: c.name.clone(),
                death_time: c.death_time,
                x: c.x,
                y: c.y,
                grid: map_size.and_then(|s| grid_pos(c.x as f64, c.y as f64, s as f64)),
            });
        }
    }
    out.sort_by_key(|d| d.death_time);
    out
}

/// Текст для командного чата (коротко: лимит чата Rust невелик).
pub fn chat_text(d: &Death, local_time: &str) -> String {
    let name: String = d.name.chars().filter(|c| !c.is_control()).take(32).collect();
    match &d.grid {
        Some(g) => format!("☠ {name} умер в {g} ({local_time})"),
        None => format!("☠ {name} умер за пределами сетки ({local_time})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn m(id: u64, alive: bool, dt: u32, x: f32, y: f32) -> Member {
        Member {
            steam_id: Some(id),
            name: Some(format!("P{id}")),
            x: Some(x),
            y: Some(y),
            is_online: Some(true),
            spawn_time: Some(1),
            is_alive: Some(alive),
            death_time: Some(dt),
        }
    }
    #[test]
    fn detects_transition_to_dead_with_grid() {
        let a = snapshot(&[m(1, true, 0, 10.0, 4380.0), m(2, true, 0, 0.0, 0.0)]);
        let b = snapshot(&[m(1, false, 500, 10.0, 4380.0), m(2, true, 0, 0.0, 0.0)]);
        let d = detect(Some(&a), &b, Some(4500));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].grid.as_deref(), Some("A0"));
        assert_eq!(chat_text(&d[0], "21:05"), "☠ P1 умер в A0 (21:05)");
        // тот же снимок ещё раз — без дублей
        assert!(detect(Some(&b), &b, Some(4500)).is_empty());
    }
    #[test]
    fn first_snapshot_and_respawn_do_not_alert() {
        let dead = snapshot(&[m(1, false, 500, 1.0, 1.0)]);
        assert!(detect(None, &dead, Some(4500)).is_empty(), "при подключении не сообщаем о старых смертях");
        let alive = snapshot(&[m(1, true, 500, 1.0, 1.0)]);
        assert!(detect(Some(&dead), &alive, Some(4500)).is_empty(), "респаун — не смерть");
        // умер снова, пока мы не видели «жив» (новое deathTime)
        let dead2 = snapshot(&[m(1, false, 900, 1.0, 1.0)]);
        assert_eq!(detect(Some(&dead), &dead2, Some(4500)).len(), 1);
    }
}
