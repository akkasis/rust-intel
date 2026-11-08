//! Идентификаторы. SteamID64 и идентификаторы провайдеров — разные типы,
//! их нельзя случайно перепутать (BattleMetrics player id ≠ SteamID64).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Базовое значение SteamID64 для индивидуальных аккаунтов (universe=1, type=1, instance=1).
pub const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// Проверенный SteamID64 индивидуального аккаунта. В JSON всегда строка.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SteamId64(String);

impl SteamId64 {
    pub fn from_u64(v: u64) -> Result<Self, IdError> {
        if v <= STEAM_ID64_BASE || v - STEAM_ID64_BASE > u32::MAX as u64 {
            return Err(IdError::OutOfRange);
        }
        Ok(Self(v.to_string()))
    }
    pub fn parse(s: &str) -> Result<Self, IdError> {
        let s = s.trim();
        if s.len() != 17 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(IdError::NotSteamId64);
        }
        let v: u64 = s.parse().map_err(|_| IdError::NotSteamId64)?;
        Self::from_u64(v)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn account_id(&self) -> u32 {
        (self.0.parse::<u64>().unwrap_or(STEAM_ID64_BASE) - STEAM_ID64_BASE) as u32
    }
    pub fn steam2(&self) -> String {
        let a = self.account_id();
        format!("STEAM_0:{}:{}", a & 1, a >> 1)
    }
    pub fn steam3(&self) -> String {
        format!("[U:1:{}]", self.account_id())
    }
    pub fn profile_url(&self) -> String {
        format!("https://steamcommunity.com/profiles/{}", self.0)
    }
}

impl TryFrom<String> for SteamId64 {
    type Error = IdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<SteamId64> for String {
    fn from(v: SteamId64) -> Self {
        v.0
    }
}
impl fmt::Display for SteamId64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Идентификатор игрока у BattleMetrics.com — НЕ SteamID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BattleMetricsPlayerId(pub String);

/// Идентификатор сервера у провайдера (`provider:external_id` в БД).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServerKey {
    pub provider: String,
    pub external_id: String,
}
impl ServerKey {
    pub fn db_id(&self) -> String {
        format!("{}:{}", self.provider, self.external_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdError {
    #[error("Это не SteamID64 (ожидается 17 цифр, начинается с 7656119)")]
    NotSteamId64,
    #[error("SteamID вне диапазона индивидуальных аккаунтов")]
    OutOfRange,
    #[error("Не удалось распознать идентификатор")]
    Unrecognized,
    #[error("Пустой ввод")]
    Empty,
    #[error("Ссылка ведёт не на steamcommunity.com")]
    ForeignHost,
}

/// Результат локального разбора пользовательского ввода.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParsedInput {
    /// Однозначный SteamID64 (из десятичного вида, ссылки, Steam2 или Steam3).
    SteamId { steam_id: SteamId64, from_format: String },
    /// Vanity URL — требуется разрешение через Steam Web API (ResolveVanityURL).
    Vanity { vanity: String },
    /// Свободный текст: может быть ником или vanity; нужна явная проверка, без угадывания.
    Ambiguous { text: String },
}

fn valid_vanity(s: &str) -> bool {
    (2..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn parse_input(raw: &str) -> Result<ParsedInput, IdError> {
    let s = raw.trim().trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>');
    if s.is_empty() {
        return Err(IdError::Empty);
    }
    // 1) Десятичный SteamID64
    if s.bytes().all(|b| b.is_ascii_digit()) {
        return SteamId64::parse(s).map(|steam_id| ParsedInput::SteamId { steam_id, from_format: "steamid64".into() });
    }
    // 2) Steam2: STEAM_X:Y:Z
    let upper = s.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("STEAM_") {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() == 3 {
            let universe: u8 = parts[0].parse().map_err(|_| IdError::Unrecognized)?;
            let y: u64 = parts[1].parse().map_err(|_| IdError::Unrecognized)?;
            let z: u64 = parts[2].parse().map_err(|_| IdError::Unrecognized)?;
            if universe > 1 || y > 1 {
                return Err(IdError::Unrecognized);
            }
            let acc = z.checked_mul(2).and_then(|v| v.checked_add(y)).ok_or(IdError::OutOfRange)?;
            let steam_id = SteamId64::from_u64(STEAM_ID64_BASE.checked_add(acc).ok_or(IdError::OutOfRange)?)?;
            return Ok(ParsedInput::SteamId { steam_id, from_format: "steam2".into() });
        }
        return Err(IdError::Unrecognized);
    }
    // 3) Steam3: [U:1:N] (скобки опциональны)
    let inner = upper.trim_start_matches('[').trim_end_matches(']');
    if let Some(n) = inner.strip_prefix("U:1:") {
        let acc: u64 = n.parse().map_err(|_| IdError::Unrecognized)?;
        let steam_id = SteamId64::from_u64(STEAM_ID64_BASE.checked_add(acc).ok_or(IdError::OutOfRange)?)?;
        return Ok(ParsedInput::SteamId { steam_id, from_format: "steam3".into() });
    }
    // 4) URL профиля
    let looks_like_url = s.contains("steamcommunity.com") || s.starts_with("http://") || s.starts_with("https://");
    if looks_like_url {
        let with_scheme = if s.starts_with("http") { s.to_string() } else { format!("https://{s}") };
        let url = url::Url::parse(&with_scheme).map_err(|_| IdError::Unrecognized)?;
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if host != "steamcommunity.com" && host != "www.steamcommunity.com" {
            return Err(IdError::ForeignHost);
        }
        let segs: Vec<&str> = url.path_segments().map(|p| p.filter(|x| !x.is_empty()).collect()).unwrap_or_default();
        return match segs.as_slice() {
            ["profiles", id, ..] => SteamId64::parse(id).map(|steam_id| ParsedInput::SteamId { steam_id, from_format: "profile_url".into() }),
            ["id", v, ..] if valid_vanity(v) => Ok(ParsedInput::Vanity { vanity: (*v).to_string() }),
            _ => Err(IdError::Unrecognized),
        };
    }
    // 5) "id/vanity" без домена
    if let Some(v) = s.strip_prefix("id/") {
        let v = v.trim_end_matches('/');
        if valid_vanity(v) {
            return Ok(ParsedInput::Vanity { vanity: v.to_string() });
        }
    }
    let text: String = s.chars().filter(|c| !c.is_control()).take(64).collect();
    Ok(ParsedInput::Ambiguous { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "76561197960287930"; // известный публичный пример из документации Valve (account 22202)

    fn sid(p: ParsedInput) -> String {
        match p {
            ParsedInput::SteamId { steam_id, .. } => steam_id.to_string(),
            other => panic!("ожидался SteamId, получено {other:?}"),
        }
    }

    #[test]
    fn parses_decimal() {
        assert_eq!(sid(parse_input(ID).unwrap()), ID);
        assert_eq!(sid(parse_input(&format!("  {ID}\n")).unwrap()), ID);
    }

    #[test]
    fn rejects_bad_decimal() {
        assert_eq!(parse_input("123"), Err(IdError::NotSteamId64));
        assert_eq!(parse_input("76561197960265728"), Err(IdError::OutOfRange));
        assert_eq!(parse_input("99999999999999999"), Err(IdError::OutOfRange));
    }

    #[test]
    fn parses_steam2_and_steam3_roundtrip() {
        assert_eq!(sid(parse_input("STEAM_0:0:11101").unwrap()), ID);
        assert_eq!(sid(parse_input("STEAM_1:0:11101").unwrap()), ID);
        assert_eq!(sid(parse_input("[U:1:22202]").unwrap()), ID);
        assert_eq!(sid(parse_input("U:1:22202").unwrap()), ID);
        let s = SteamId64::parse(ID).unwrap();
        assert_eq!(s.steam2(), "STEAM_0:0:11101");
        assert_eq!(s.steam3(), "[U:1:22202]");
        assert!(parse_input("STEAM_0:2:5").is_err());
    }

    #[test]
    fn parses_urls() {
        assert_eq!(sid(parse_input(&format!("https://steamcommunity.com/profiles/{ID}/")).unwrap()), ID);
        assert_eq!(sid(parse_input(&format!("steamcommunity.com/profiles/{ID}")).unwrap()), ID);
        assert_eq!(parse_input("https://steamcommunity.com/id/gabelogannewell/").unwrap(), ParsedInput::Vanity { vanity: "gabelogannewell".into() });
        assert_eq!(parse_input("id/some_name").unwrap(), ParsedInput::Vanity { vanity: "some_name".into() });
        assert_eq!(parse_input("https://evil.example/profiles/1"), Err(IdError::ForeignHost));
        assert_eq!(parse_input("https://steamcommunity.com/groups/x"), Err(IdError::Unrecognized));
    }

    #[test]
    fn ambiguous_text_is_not_guessed() {
        assert!(matches!(parse_input("Ворон").unwrap(), ParsedInput::Ambiguous { .. }));
        assert_eq!(parse_input("   "), Err(IdError::Empty));
    }

    #[test]
    fn json_is_string() {
        let s = SteamId64::parse(ID).unwrap();
        assert_eq!(serde_json::to_string(&s).unwrap(), format!("\"{ID}\""));
        let back: SteamId64 = serde_json::from_str(&format!("\"{ID}\"")).unwrap();
        assert_eq!(back, s);
        assert!(serde_json::from_str::<SteamId64>("76561197960287930").is_err());
    }
}
