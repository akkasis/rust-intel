-- Rust+: спаренные серверы. Токен игрока хранится в системном хранилище секретов, не здесь.
CREATE TABLE rustplus_servers (
    id                  TEXT PRIMARY KEY,          -- ip:port
    name                TEXT NOT NULL,
    ip                  TEXT NOT NULL,
    port                INTEGER NOT NULL,
    player_id           TEXT NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 1,
    death_alerts_chat   INTEGER NOT NULL DEFAULT 1, -- писать в командный чат
    death_alerts_local  INTEGER NOT NULL DEFAULT 1, -- уведомления Windows/Telegram
    paired_at           TEXT NOT NULL,
    last_connected_at   TEXT,
    last_error          TEXT
);
