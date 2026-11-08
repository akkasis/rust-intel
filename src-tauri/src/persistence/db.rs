use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Ошибка базы данных: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Ошибка миграции: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("Ошибка файловой системы: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn open(path: &Path) -> Result<SqlitePool, DbError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let existed = path.exists();
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
    if existed {
        backup_before_migration(&pool, path).await?;
    }
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

pub async fn open_memory() -> Result<SqlitePool, DbError> {
    let opts = SqliteConnectOptions::new().in_memory(true).foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// Если есть неприменённые миграции — сначала делаем резервную копию.
async fn backup_before_migration(pool: &SqlitePool, path: &Path) -> Result<(), DbError> {
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1").fetch_all(pool).await.unwrap_or_default();
    let pending = MIGRATOR.iter().any(|m| !applied.contains(&m.version));
    if pending && !applied.is_empty() {
        let backup = backup_path(path, "pre-migration");
        backup_to(pool, &backup).await?;
        tracing::info!(backup = %backup.display(), "резервная копия перед миграцией");
    }
    Ok(())
}

pub fn backup_path(db: &Path, label: &str) -> PathBuf {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let stem = db.file_stem().and_then(|s| s.to_str()).unwrap_or("rid");
    db.with_file_name(format!("{stem}.{label}.{stamp}.sqlite"))
}

pub async fn backup_to(pool: &SqlitePool, target: &Path) -> Result<(), DbError> {
    if target.exists() {
        std::fs::remove_file(target)?;
    }
    sqlx::query("VACUUM INTO ?").bind(target.to_string_lossy().to_string()).execute(pool).await?;
    Ok(())
}

pub async fn integrity_check(pool: &SqlitePool) -> Result<String, DbError> {
    let r: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(pool).await?;
    Ok(r)
}

pub async fn schema_version(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1").fetch_one(pool).await.unwrap_or(0)
}
