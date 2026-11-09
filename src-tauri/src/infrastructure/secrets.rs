//! Хранилище секретов: Windows Credential Manager / macOS Keychain через `keyring`.
//! Если системное хранилище недоступно — секрет держится только в памяти до закрытия приложения.

use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE: &str = "RustIntelligenceDesktop";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretKey {
    SteamApiKey,
    BattlemetricsToken,
    BattlemetricsRuKey,
    TelegramBotToken,
}

impl SecretKey {
    pub fn account(&self) -> &'static str {
        match self {
            Self::SteamApiKey => "steam_api_key",
            Self::BattlemetricsToken => "battlemetrics_token",
            Self::BattlemetricsRuKey => "battlemetrics_ru_key",
            Self::TelegramBotToken => "telegram_bot_token",
        }
    }
    pub const ALL: [SecretKey; 4] = [Self::SteamApiKey, Self::BattlemetricsToken, Self::BattlemetricsRuKey, Self::TelegramBotToken];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretBackend {
    OsKeyring,
    MemoryOnly,
}

pub struct SecretStore {
    memory: Mutex<HashMap<SecretKey, String>>,
    raw: Mutex<HashMap<String, String>>,
    use_keyring: bool,
}

impl SecretStore {
    pub fn new() -> Self {
        let use_keyring = keyring::Entry::store_status().is_ok();
        if !use_keyring {
            tracing::warn!("системное хранилище секретов недоступно — ключи будут храниться только в памяти");
        }
        Self { memory: Mutex::new(HashMap::new()), raw: Mutex::new(HashMap::new()), use_keyring }
    }
    /// Для тестов: без обращения к системному хранилищу.
    pub fn memory_only() -> Self {
        Self { memory: Mutex::new(HashMap::new()), raw: Mutex::new(HashMap::new()), use_keyring: false }
    }
    pub fn backend(&self) -> SecretBackend {
        if self.use_keyring { SecretBackend::OsKeyring } else { SecretBackend::MemoryOnly }
    }

    pub fn get(&self, key: SecretKey) -> Option<String> {
        if let Some(v) = self.memory.lock().ok()?.get(&key).cloned() {
            return Some(v);
        }
        if !self.use_keyring {
            return None;
        }
        let v = keyring::Entry::new(SERVICE, key.account()).ok()?.get_password().ok()?;
        if let Ok(mut m) = self.memory.lock() {
            m.insert(key, v.clone());
        }
        Some(v)
    }

    pub fn set(&self, key: SecretKey, value: &str) -> Result<SecretBackend, String> {
        let value = value.trim();
        if value.is_empty() || value.len() > 512 || value.chars().any(|c| c.is_control()) {
            return Err("Недопустимое значение секрета".into());
        }
        if let Ok(mut m) = self.memory.lock() {
            m.insert(key, value.to_string());
        }
        if self.use_keyring {
            match keyring::Entry::new(SERVICE, key.account()).and_then(|e| e.set_password(value)) {
                Ok(()) => return Ok(SecretBackend::OsKeyring),
                Err(e) => tracing::warn!(error = %e, "не удалось сохранить секрет в системное хранилище"),
            }
        }
        Ok(SecretBackend::MemoryOnly)
    }

    /// Произвольный секрет по имени (для токенов Rust+). Хранится так же, как ключи API.
    pub fn get_raw(&self, account: &str) -> Option<String> {
        if let Some(v) = self.raw.lock().ok()?.get(account).cloned() {
            return Some(v);
        }
        if !self.use_keyring {
            return None;
        }
        let v = keyring::Entry::new(SERVICE, account).ok()?.get_password().ok()?;
        if let Ok(mut m) = self.raw.lock() {
            m.insert(account.to_string(), v.clone());
        }
        Some(v)
    }

    pub fn set_raw(&self, account: &str, value: &str) -> Result<SecretBackend, String> {
        if value.is_empty() || value.len() > 2000 {
            return Err("Недопустимое значение секрета".into());
        }
        if let Ok(mut m) = self.raw.lock() {
            m.insert(account.to_string(), value.to_string());
        }
        if self.use_keyring {
            match keyring::Entry::new(SERVICE, account).and_then(|e| e.set_password(value)) {
                Ok(()) => return Ok(SecretBackend::OsKeyring),
                Err(e) => tracing::warn!(error = %e, "не удалось сохранить секрет"),
            }
        }
        Ok(SecretBackend::MemoryOnly)
    }

    pub fn delete_raw(&self, account: &str) {
        if let Ok(mut m) = self.raw.lock() {
            m.remove(account);
        }
        if self.use_keyring
            && let Ok(e) = keyring::Entry::new(SERVICE, account)
        {
            let _ = e.delete_credential();
        }
    }

    pub fn delete(&self, key: SecretKey) {
        if let Ok(mut m) = self.memory.lock() {
            m.remove(&key);
        }
        if self.use_keyring
            && let Ok(e) = keyring::Entry::new(SERVICE, key.account())
        {
            let _ = e.delete_credential();
        }
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}
