//! Удаление секретов из строк для логов и сообщений об ошибках.

use regex::Regex;
use std::sync::LazyLock;

static PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"(?i)(key|token|access_token|apikey)=([^&\s]+)").unwrap(), "$1=[REDACTED]"),
        (Regex::new(r"(?i)(authorization:\s*bearer\s+)\S+").unwrap(), "$1[REDACTED]"),
        (Regex::new(r"(?i)bearer\s+[A-Za-z0-9._\-]+").unwrap(), "Bearer [REDACTED]"),
        (Regex::new(r"bot\d+:[A-Za-z0-9_\-]+").unwrap(), "bot[REDACTED]"),
        (Regex::new(r"\b[0-9A-F]{32}\b").unwrap(), "[REDACTED_KEY]"),
    ]
});

pub fn redact(s: &str) -> String {
    let mut out = s.to_string();
    for (re, rep) in PATTERNS.iter() {
        out = re.replace_all(&out, *rep).into_owned();
    }
    out
}

/// Маска для отображения факта наличия секрета (последние 4 символа).
pub fn mask(secret: &str) -> String {
    let n = secret.chars().count();
    if n <= 4 {
        return "••••".into();
    }
    let tail: String = secret.chars().skip(n - 4).collect();
    format!("••••{tail}")
}

/// Очистка строк, пришедших извне: убираем управляющие символы, ограничиваем длину.
pub fn sanitize_text(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == ' ').take(max).collect::<String>().trim().to_string()
}

/// Разрешаем только https-ссылки на известные хосты.
pub fn sanitize_url(s: &str, allowed_hosts: &[&str]) -> Option<String> {
    let u = url::Url::parse(s).ok()?;
    if u.scheme() != "https" {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    allowed_hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}"))).then(|| u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_keys() {
        let s = redact("GET https://api.steampowered.com/x?key=ABCDEF0123456789ABCDEF0123456789&steamids=1");
        assert!(!s.contains("ABCDEF0123"));
        assert!(redact("https://api.telegram.org/bot123456:AAH-xyz/sendMessage").contains("bot[REDACTED]"));
        assert!(!redact("Authorization: Bearer eyJabc.def").contains("eyJabc"));
    }
    #[test]
    fn url_allowlist() {
        assert!(sanitize_url("https://avatars.steamstatic.com/a.jpg", &["steamstatic.com"]).is_some());
        assert!(sanitize_url("http://avatars.steamstatic.com/a.jpg", &["steamstatic.com"]).is_none());
        assert!(sanitize_url("https://evilsteamstatic.com/a.jpg", &["steamstatic.com"]).is_none());
        assert!(sanitize_url("javascript:alert(1)", &["steamstatic.com"]).is_none());
    }
}
