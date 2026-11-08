-- Друзья Steam (для поиска настоящих тиммейтов) и данные для автопривязки BattleMetrics.

CREATE TABLE steam_friends (
    steam_id      TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    friend_id     TEXT NOT NULL CHECK (length(friend_id) = 17),
    persona_name  TEXT,
    avatar_url    TEXT,
    visibility    TEXT,
    in_rust       INTEGER,
    friend_since  TEXT,
    fetched_at    TEXT NOT NULL,
    PRIMARY KEY (steam_id, friend_id)
);
CREATE INDEX idx_steam_friends_friend ON steam_friends(friend_id);

ALTER TABLE players ADD COLUMN friends_status TEXT;        -- ok | private | error
ALTER TABLE players ADD COLUMN friends_fetched_at TEXT;
ALTER TABLE players ADD COLUMN last_game_server TEXT;      -- ip:port из Steam, когда игрок в Rust
ALTER TABLE players ADD COLUMN last_game_server_at TEXT;

-- Попытки автопривязки BM (чтобы не долбить API и показывать, почему не нашлось).
CREATE TABLE bm_link_attempts (
    steam_id      TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    attempted_at  TEXT NOT NULL,
    method        TEXT NOT NULL,
    result        TEXT NOT NULL,
    PRIMARY KEY (steam_id, attempted_at, method)
);
