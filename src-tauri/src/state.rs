//! Состояние приложения и контекст для сервисов.

use crate::config::{self, AppConfig, AppSettings, Mode};
use crate::error::{AppError, AppResult};
use crate::infrastructure::http::HttpClient;
use crate::infrastructure::secrets::{SecretKey, SecretStore};
use crate::persistence::db;
use crate::providers::{battlemetrics::BattleMetricsProvider, bmru::BmRuProvider, steam::SteamProvider};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

#[derive(Clone)]
pub struct Ctx {
    pub pool: SqlitePool,
    pub mode: Mode,
    pub settings: AppSettings,
    pub steam: Arc<SteamProvider>,
    pub bm: Arc<BattleMetricsProvider>,
    pub bmru: Arc<BmRuProvider>,
    pub secrets: Arc<SecretStore>,
    pub http: HttpClient,
}

impl Ctx {
    pub fn is_demo(&self) -> bool {
        self.mode == Mode::Demo
    }
    pub fn tz(&self) -> chrono_tz::Tz {
        self.settings.timezone.parse().unwrap_or(chrono_tz::UTC)
    }
}

struct Inner {
    config: AppConfig,
    pools: HashMap<Mode, SqlitePool>,
}

pub struct AppState {
    pub data_dir: PathBuf,
    inner: RwLock<Inner>,
    pub secrets: Arc<SecretStore>,
    pub http: HttpClient,
    pub steam: Arc<SteamProvider>,
    pub bm: Arc<BattleMetricsProvider>,
    pub bmru: Arc<BmRuProvider>,
    pub tracker_wake: Arc<Notify>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, secrets: Arc<SecretStore>) -> Self {
        let http = HttpClient::new();
        let s1 = secrets.clone();
        let s2 = secrets.clone();
        let s3 = secrets.clone();
        let steam = Arc::new(SteamProvider::new(http.clone(), Arc::new(move || s1.get(SecretKey::SteamApiKey))));
        let bm = Arc::new(BattleMetricsProvider::new(http.clone(), Arc::new(move || s2.get(SecretKey::BattlemetricsToken))));
        let bmru = Arc::new(BmRuProvider::new(http.clone(), Arc::new(move || s3.get(SecretKey::BattlemetricsRuKey))));
        let config = config::load(&data_dir);
        Self { data_dir, inner: RwLock::new(Inner { config, pools: HashMap::new() }), secrets, http, steam, bm, bmru, tracker_wake: Arc::new(Notify::new()) }
    }

    pub async fn config(&self) -> AppConfig {
        self.inner.read().await.config.clone()
    }

    pub fn db_path(&self, mode: Mode) -> PathBuf {
        self.data_dir.join(mode.db_file())
    }

    async fn pool_for(&self, mode: Mode) -> AppResult<SqlitePool> {
        if let Some(p) = self.inner.read().await.pools.get(&mode) {
            return Ok(p.clone());
        }
        let pool = db::open(&self.db_path(mode)).await?;
        if mode == Mode::Demo {
            crate::providers::demo::seed(&pool, chrono::Utc::now()).await?;
        }
        self.inner.write().await.pools.insert(mode, pool.clone());
        Ok(pool)
    }

    pub async fn ctx(&self) -> AppResult<Ctx> {
        let (mode, settings) = {
            let g = self.inner.read().await;
            (g.config.mode.ok_or(AppError::FirstRun)?, g.config.settings.clone())
        };
        Ok(Ctx {
            pool: self.pool_for(mode).await?,
            mode,
            settings,
            steam: self.steam.clone(),
            bm: self.bm.clone(),
            bmru: self.bmru.clone(),
            secrets: self.secrets.clone(),
            http: self.http.clone(),
        })
    }

    pub async fn set_mode(&self, mode: Mode) -> AppResult<()> {
        self.pool_for(mode).await?;
        let mut g = self.inner.write().await;
        g.config.mode = Some(mode);
        config::save(&self.data_dir, &g.config)?;
        drop(g);
        self.tracker_wake.notify_one();
        Ok(())
    }

    pub async fn update_settings(&self, s: AppSettings) -> AppResult<AppSettings> {
        let s = s.validate().map_err(AppError::validation)?;
        let mut g = self.inner.write().await;
        g.config.settings = s.clone();
        config::save(&self.data_dir, &g.config)?;
        crate::tray::set_close_to_tray(s.close_to_tray);
        drop(g);
        self.tracker_wake.notify_one();
        Ok(s)
    }

    /// Закрывает пул режима и удаляет файл БД (полная очистка).
    pub async fn drop_database(&self, mode: Mode) -> AppResult<()> {
        if let Some(p) = self.inner.write().await.pools.remove(&mode) {
            p.close().await;
        }
        for suffix in ["", "-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{suffix}", self.db_path(mode).display()));
            if p.exists() {
                std::fs::remove_file(p)?;
            }
        }
        Ok(())
    }

    pub async fn replace_database(&self, mode: Mode, from: &std::path::Path) -> AppResult<()> {
        // Проверяем, что файл — корректная БД RID, прежде чем заменять.
        let probe = db::open(from).await?;
        let ok = db::integrity_check(&probe).await?;
        probe.close().await;
        if ok != "ok" {
            return Err(AppError::validation(format!("Резервная копия повреждена: {ok}")));
        }
        let current = self.ctx().await.ok().map(|c| c.pool);
        if let Some(pool) = current {
            let safety = db::backup_path(&self.db_path(mode), "before-restore");
            db::backup_to(&pool, &safety).await?;
        }
        self.drop_database(mode).await?;
        std::fs::copy(from, self.db_path(mode))?;
        self.pool_for(mode).await?;
        Ok(())
    }
}
