use crate::providers::error::ProviderError;
use serde::Serialize;

/// Ошибка на границе команд Tauri. Сериализуется в `{kind, message, ...}`; секретов не содержит.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppError {
    #[error("{message}")]
    Validation { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("{provider}: {error}")]
    Provider { provider: String, error: ProviderError, message: String },
    #[error("Ошибка базы данных: {message}")]
    Db { message: String },
    #[error("Сначала выберите режим работы")]
    FirstRun,
    #[error("{message}")]
    Io { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl AppError {
    pub fn validation(m: impl Into<String>) -> Self {
        Self::Validation { message: m.into() }
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::NotFound { message: m.into() }
    }
    pub fn provider(p: &str, e: ProviderError) -> Self {
        Self::Provider { provider: p.into(), message: e.to_string(), error: e }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "db");
        Self::Db { message: crate::infrastructure::redact::redact(&e.to_string()) }
    }
}
impl From<crate::persistence::db::DbError> for AppError {
    fn from(e: crate::persistence::db::DbError) -> Self {
        Self::Db { message: crate::infrastructure::redact::redact(&e.to_string()) }
    }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Io { message: e.to_string() }
    }
}
impl From<crate::domain::ids::IdError> for AppError {
    fn from(e: crate::domain::ids::IdError) -> Self {
        Self::Validation { message: e.to_string() }
    }
}

pub type AppResult<T> = Result<T, AppError>;
