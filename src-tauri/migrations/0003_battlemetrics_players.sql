-- BattleMetrics как основной источник: игроки BM (могут не иметь известного SteamID),
-- их серверы, сессии из наблюдения за серверами и из API, наблюдаемые серверы.

CREATE TABLE bm_players (
    bm_id       TEXT PRIMARY KEY CHECK (length(bm_id) BETWEEN 1 AND 20),
    name        TEXT,
    steam_id    TEXT,            -- только если известен (привязка или RCON-доступ)
    tracked     INTEGER NOT NULL DEFAULT 0,
    first_seen  TEXT,
    last_seen   TEXT,
    fetched_at  TEXT,            -- последний запрос /players/{id}
    is_demo     INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL
);
CREATE INDEX idx_bm_players_name ON bm_players(name);

CREATE TABLE bm_player_names (
    bm_id       TEXT NOT NULL REFERENCES bm_players(bm_id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    first_seen  TEXT NOT NULL,
    last_seen   TEXT NOT NULL,
    PRIMARY KEY (bm_id, name)
);

CREATE TABLE bm_player_servers (
    bm_id             TEXT NOT NULL REFERENCES bm_players(bm_id) ON DELETE CASCADE,
    server_id         TEXT NOT NULL,
    server_name       TEXT,
    online            INTEGER NOT NULL DEFAULT 0,
    first_seen        TEXT,
    last_seen         TEXT,
    time_played_secs  INTEGER,
    fetched_at        TEXT NOT NULL,
    PRIMARY KEY (bm_id, server_id)
);

-- Сессии игрока на сервере. start_known/end_known = переход наблюдался между двумя последовательными опросами
-- (или сообщён API). Иначе граница — лишь «не позже/не раньше».
CREATE TABLE bm_sessions (
    id           INTEGER PRIMARY KEY,
    bm_id        TEXT NOT NULL REFERENCES bm_players(bm_id) ON DELETE CASCADE,
    server_id    TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('server_watch','bm_api','demo')),
    start_lower  TEXT NOT NULL,
    start_upper  TEXT NOT NULL,
    start_known  INTEGER NOT NULL,
    end_lower    TEXT NOT NULL,
    end_upper    TEXT,
    end_known    INTEGER NOT NULL DEFAULT 0,
    is_open      INTEGER NOT NULL DEFAULT 0,
    external_id  TEXT,
    UNIQUE (bm_id, server_id, start_upper)
);
CREATE INDEX idx_bm_sessions_player ON bm_sessions(bm_id, start_upper);
CREATE INDEX idx_bm_sessions_server ON bm_sessions(server_id, start_upper);
CREATE INDEX idx_bm_sessions_open ON bm_sessions(server_id, is_open);

CREATE TABLE server_watch (
    server_id        TEXT PRIMARY KEY,
    enabled          INTEGER NOT NULL DEFAULT 1,
    added_at         TEXT NOT NULL,
    last_polled_at   TEXT,
    last_success_at  TEXT,
    last_error       TEXT,
    last_online      INTEGER
);

-- Окна, когда сервер реально наблюдался (знаменатель для активности игроков BM).
CREATE TABLE server_coverage (
    server_id  TEXT NOT NULL,
    start_at   TEXT NOT NULL,
    end_at     TEXT NOT NULL,
    PRIMARY KEY (server_id, start_at)
);
