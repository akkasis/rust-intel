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

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv1a(data: &[u8]) -> u64 {
        data.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
    }

    /// Выпущенные миграции нельзя менять: иначе база пользователя не откроется после обновления.
    /// Новые изменения схемы — только новыми файлами 0003_…, 0004_….
    #[test]
    fn released_migrations_are_frozen() {
        let m1 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations/0001_init.sql")).unwrap();
        assert_eq!(fnv1a(&m1), 0x3792b5b23031bf13, "0001_init.sql изменён — создайте новую миграцию вместо правки");
        let m2 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations/0002_player_server_stats.sql")).unwrap();
        assert_eq!(fnv1a(&m2), 0x452fc54d638cc1f3, "0002 изменён — создайте новую миграцию вместо правки");
        let m3 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations/0003_battlemetrics_players.sql")).unwrap();
        assert_eq!(fnv1a(&m3), 0x6731d05fb994c1ea, "0003 изменён — создайте новую миграцию вместо правки");
    }

    /// Обновление приложения: база предыдущей версии (только миграция 0001) открывается новой версией,
    /// данные сохраняются, перед миграцией создаётся резервная копия.
    #[tokio::test]
    async fn upgrade_from_previous_version_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let old_migrations = dir.path().join("old_migrations");
        std::fs::create_dir_all(&old_migrations).unwrap();
        std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations/0001_init.sql"), old_migrations.join("0001_init.sql")).unwrap();
        let db = dir.path().join("rid-live.sqlite");
        {
            let opts = SqliteConnectOptions::new().filename(&db).create_if_missing(true).foreign_keys(true);
            let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
            sqlx::migrate::Migrator::new(old_migrations.as_path()).await.unwrap().run(&pool).await.unwrap();
            sqlx::query("INSERT INTO players(steam_id, persona_name, created_at, updated_at) VALUES ('76561197960287930', 'Old', 'x', 'x')")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO squads(name, created_at, updated_at) VALUES ('Клан', 'x', 'x')").execute(&pool).await.unwrap();
            pool.close().await;
        }
        let pool = open(&db).await.unwrap();
        let name: String = sqlx::query_scalar("SELECT persona_name FROM players").fetch_one(&pool).await.unwrap();
        assert_eq!(name, "Old");
        let squads: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM squads").fetch_one(&pool).await.unwrap();
        assert_eq!(squads, 1);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM player_server_stats").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 0, "новая таблица создана");
        assert_eq!(schema_version(&pool).await, MIGRATOR.iter().map(|m| m.version).max().unwrap());
        let backups =
            std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().contains("pre-migration")).count();
        assert_eq!(backups, 1, "резервная копия перед миграцией");
        // повторное открытие той же версией — без новых бэкапов
        pool.close().await;
        let pool = open(&db).await.unwrap();
        pool.close().await;
        let backups2 =
            std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().contains("pre-migration")).count();
        assert_eq!(backups2, 1);
    }
}
