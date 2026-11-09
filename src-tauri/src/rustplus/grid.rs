//! Квадрат карты Rust по координатам (как на игровой карте: буквы слева направо, цифры сверху вниз с 0).
//! Формула сверена с rustplusplus (src/util/map.js): ячейка 146.25 м, «скорректированный» размер карты.

pub const GRID: f64 = 146.25;

pub fn corrected_map_size(map_size: f64) -> f64 {
    let rem = map_size % GRID;
    if rem < 120.0 { map_size - rem } else { map_size + (GRID - rem) }
}

pub fn letters(mut n: u32) -> String {
    // 1 → A, 26 → Z, 27 → AA
    let mut out = Vec::new();
    while n > 0 {
        let m = (n - 1) % 26;
        out.push((b'A' + m as u8) as char);
        n = (n - 1) / 26;
    }
    out.iter().rev().collect()
}

/// None — точка вне сетки (море, за краем карты).
pub fn grid_pos(x: f64, y: f64, map_size: f64) -> Option<String> {
    let size = corrected_map_size(map_size);
    if !(0.0..=size).contains(&x) || !(0.0..=size).contains(&y) {
        return None;
    }
    let cells = (size / GRID).floor() as i64;
    let col = ((x / GRID).floor() as i64).min(cells - 1) + 1;
    let row_from_bottom = ((y / GRID).floor() as i64).min(cells - 1) + 1;
    let number = cells - row_from_bottom;
    Some(format!("{}{}", letters(col as u32), number.max(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letters_work() {
        assert_eq!(letters(1), "A");
        assert_eq!(letters(26), "Z");
        assert_eq!(letters(27), "AA");
        assert_eq!(letters(28), "AB");
    }
    #[test]
    fn grid_matches_game_layout() {
        // карта 4500: 30 ячеек (4500 % 146.25 = 112.5 < 120 → 4387.5)
        assert_eq!(corrected_map_size(4500.0), 4387.5);
        assert_eq!(grid_pos(10.0, 4380.0, 4500.0).as_deref(), Some("A0"), "левый верхний угол");
        assert_eq!(grid_pos(10.0, 10.0, 4500.0).as_deref(), Some("A29"), "левый нижний");
        assert_eq!(grid_pos(4380.0, 4380.0, 4500.0).as_deref(), Some("AD0"), "правый верхний");
        assert_eq!(grid_pos(2000.0, 2000.0, 4500.0).as_deref(), Some("N16"));
        assert_eq!(grid_pos(-5.0, 100.0, 4500.0), None);
    }
}
