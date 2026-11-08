-- Rust Intelligence Desktop — начальная схема. Все моменты времени — UTC ISO-8601 (RFC 3339).

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE players (
    steam_id            TEXT PRIMARY KEY CHECK (length(steam_id) = 17),
    persona_name        TEXT,
    avatar_url          TEXT,
    profile_url         TEXT,
    visibility          TEXT NOT NULL DEFAULT 'unknown', -- public | private | friends_only | unknown
    account_created_at  TEXT,
    country_code        TEXT,
    rust_hours          REAL,
    rust_hours_status   TEXT NOT NULL DEFAULT 'unknown', -- verified | hidden | unknown | not_configured
    steam_in_game       INTEGER,                          -- 1 = Steam сообщает gameid Rust
    steam_status        TEXT NOT NULL DEFAULT 'not_configured',
    steam_fetched_at    TEXT,
    is_demo             INTEGER NOT NULL DEFAULT 0,
    favorite            INTEGER NOT NULL DEFAULT 0,
    notes               TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
CREATE INDEX idx_players_name ON players(persona_name);
CREATE INDEX idx_players_fav ON players(favorite);

-- Связь SteamID64 ↔ сущность провайдера (BattleMetrics player id и т.п.). Никогда не угадывается.
CREATE TABLE provider_links (
    id              INTEGER PRIMARY KEY,
    steam_id        TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,
    external_id     TEXT NOT NULL,
    match_evidence  TEXT NOT NULL, -- exact_identifier | user_confirmed | demo
    confidence      TEXT NOT NULL, -- high | medium
    source_url      TEXT,
    linked_at       TEXT NOT NULL,
    UNIQUE (provider, external_id),
    UNIQUE (steam_id, provider)
);

CREATE TABLE aliases (
    id             INTEGER PRIMARY KEY,
    steam_id       TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    source         TEXT NOT NULL,
    first_seen_at  TEXT NOT NULL,
    last_seen_at   TEXT NOT NULL,
    UNIQUE (steam_id, name, source)
);

CREATE TABLE bans (
    id               INTEGER PRIMARY KEY,
    steam_id         TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    provider         TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('vac','game','community','economy','project')),
    project_name     TEXT NOT NULL DEFAULT '',
    count            INTEGER,
    active           INTEGER NOT NULL,
    days_since_last  INTEGER,
    reason           TEXT,
    revoked          INTEGER,
    source_url       TEXT,
    observed_at      TEXT NOT NULL,
    UNIQUE (steam_id, provider, kind, project_name)
);

CREATE TABLE servers (
    id            TEXT PRIMARY KEY,      -- provider:external_id
    provider      TEXT NOT NULL,
    external_id   TEXT NOT NULL,
    name          TEXT NOT NULL,
    address       TEXT,
    country       TEXT,
    map           TEXT,
    players       INTEGER,
    max_players   INTEGER,
    queue         INTEGER,
    online        INTEGER,
    trust         TEXT,
    last_wipe_at  TEXT,
    next_wipe_at  TEXT,
    followed      INTEGER NOT NULL DEFAULT 0,
    source_url    TEXT,
    fetched_at    TEXT NOT NULL,
    is_demo       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_servers_followed ON servers(followed);

CREATE TABLE server_population (
    server_id  TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    t          TEXT NOT NULL,
    players    REAL NOT NULL,
    source     TEXT NOT NULL,
    PRIMARY KEY (server_id, t)
);

-- Сырые наблюдения (опросы). Unknown = источник не ответил (НЕ офлайн).
CREATE TABLE observations (
    id                 INTEGER PRIMARY KEY,
    steam_id           TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    provider           TEXT NOT NULL,
    observed_at        TEXT NOT NULL,
    state              TEXT NOT NULL CHECK (state IN ('online','offline','unknown')),
    server_id          TEXT,
    poll_interval_secs INTEGER NOT NULL,
    UNIQUE (steam_id, provider, observed_at)
);
CREATE INDEX idx_obs_player_time ON observations(steam_id, observed_at);

-- Сессии: восстановленные из опросов (inferred) или сообщённые провайдером.
CREATE TABLE sessions (
    id           INTEGER PRIMARY KEY,
    steam_id     TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    provider     TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('inferred_from_polls','provider_reported')),
    server_id    TEXT,
    start_lower  TEXT NOT NULL,
    start_upper  TEXT NOT NULL,
    end_lower    TEXT NOT NULL,
    end_upper    TEXT,
    is_open      INTEGER NOT NULL DEFAULT 0,
    external_id  TEXT,
    UNIQUE (steam_id, provider, start_upper)
);
CREATE INDEX idx_sessions_player_time ON sessions(steam_id, start_upper);

-- Окна, в которых источник действительно наблюдал игрока (знаменатель аналитики).
CREATE TABLE coverage_windows (
    id        INTEGER PRIMARY KEY,
    steam_id  TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    provider  TEXT NOT NULL,
    start_at  TEXT NOT NULL,
    end_at    TEXT NOT NULL,
    UNIQUE (steam_id, provider, start_at)
);
CREATE INDEX idx_cov_player_time ON coverage_windows(steam_id, start_at);

CREATE TABLE tracked_players (
    steam_id    TEXT PRIMARY KEY REFERENCES players(steam_id) ON DELETE CASCADE,
    enabled     INTEGER NOT NULL DEFAULT 1,
    notify      INTEGER NOT NULL DEFAULT 1,
    added_at    TEXT NOT NULL,
    last_state  TEXT,            -- online | offline | unknown
    last_server TEXT,
    last_polled_at TEXT,
    last_success_at TEXT
);

CREATE TABLE events (
    id              INTEGER PRIMARY KEY,
    steam_id        TEXT REFERENCES players(steam_id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,
    kind            TEXT NOT NULL,
    dedup_key       TEXT NOT NULL UNIQUE,
    occurred_lower  TEXT NOT NULL,
    occurred_upper  TEXT NOT NULL,
    approximate     INTEGER NOT NULL DEFAULT 1,
    server_id       TEXT,
    details         TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    is_read         INTEGER NOT NULL DEFAULT 0,
    notified        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_events_time ON events(occurred_upper DESC);
CREATE INDEX idx_events_player ON events(steam_id, occurred_upper DESC);

CREATE TABLE squads (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 64),
    color       TEXT NOT NULL DEFAULT '#2dd4bf',
    notes       TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE squad_members (
    squad_id    INTEGER NOT NULL REFERENCES squads(id) ON DELETE CASCADE,
    steam_id    TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    provenance  TEXT NOT NULL DEFAULT 'manual', -- manual | accepted_suggestion | demo
    added_at    TEXT NOT NULL,
    PRIMARY KEY (squad_id, steam_id)
);

CREATE TABLE provider_health (
    provider              TEXT PRIMARY KEY,
    status                TEXT NOT NULL,     -- ok | degraded | down | not_configured | not_tested | unsupported
    last_success_at       TEXT,
    last_error_at         TEXT,
    last_error_kind       TEXT,
    last_error            TEXT,
    consecutive_failures  INTEGER NOT NULL DEFAULT 0,
    checked_at            TEXT NOT NULL
);

CREATE TABLE search_history (
    id           INTEGER PRIMARY KEY,
    query        TEXT NOT NULL,
    steam_id     TEXT,
    searched_at  TEXT NOT NULL
);
