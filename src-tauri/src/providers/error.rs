use serde::Serialize;

/// Типизированные ошибки провайдеров.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderError {
    #[error("Источник не настроен: {hint}")]
    NotConfigured { hint: String },
    #[error("Доступ запрещён (HTTP {status}): {detail}")]
    Forbidden { status: u16, detail: String },
    #[error("Превышен лимит запросов")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("Источник недоступен: {detail}")]
    Unavailable { detail: String, retry_after_secs: Option<u64> },
    #[error("Возможность не поддерживается источником: {capability}")]
    Unsupported { capability: String },
    #[error("Не найдено: {what}")]
    NotFound { what: String },
    #[error("Превышено время ожидания")]
    Timeout,
    #[error("Некорректные данные от источника: {detail}")]
    InvalidData { detail: String },
}

impl ProviderError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotConfigured { .. } => "not_configured",
            Self::Forbidden { .. } => "forbidden",
            Self::RateLimited { .. } => "rate_limited",
            Self::Unavailable { .. } => "unavailable",
            Self::Unsupported { .. } => "unsupported",
            Self::NotFound { .. } => "not_found",
            Self::Timeout => "timeout",
            Self::InvalidData { .. } => "invalid_data",
        }
    }
    /// Сбой источника (а не факт об игроке): такие ошибки никогда не превращаются в «офлайн».
    pub fn is_outage(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Unavailable { .. } | Self::Timeout | Self::InvalidData { .. } | Self::Forbidden { .. })
    }
    pub fn unsupported(cap: &str) -> Self {
        Self::Unsupported { capability: cap.into() }
    }
}
