-- Статистика игрока по серверам от провайдера (BattleMetrics: время на сервере, первый/последний раз, онлайн).
CREATE TABLE player_server_stats (
    steam_id          TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    server_id         TEXT NOT NULL,
    provider          TEXT NOT NULL,
    server_name       TEXT,
    online            INTEGER NOT NULL DEFAULT 0,
    first_seen        TEXT,
    last_seen         TEXT,
    time_played_secs  INTEGER,
    fetched_at        TEXT NOT NULL,
    PRIMARY KEY (steam_id, server_id)
);
CREATE INDEX idx_pss_server ON player_server_stats(server_id);
