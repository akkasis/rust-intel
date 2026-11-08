-- Кланы из игроков BattleMetrics и история вайпов серверов.

CREATE TABLE bm_clans (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 64),
    color       TEXT NOT NULL DEFAULT '#2dd4bf',
    notify      INTEGER NOT NULL DEFAULT 1,   -- уведомлять «клан заходит»
    min_gather  INTEGER NOT NULL DEFAULT 2,   -- сколько участников должно зайти за 10 минут
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE bm_clan_members (
    clan_id     INTEGER NOT NULL REFERENCES bm_clans(id) ON DELETE CASCADE,
    bm_id       TEXT NOT NULL REFERENCES bm_players(bm_id) ON DELETE CASCADE,
    provenance  TEXT NOT NULL DEFAULT 'manual',  -- manual | teammate_suggestion | demo
    added_at    TEXT NOT NULL,
    PRIMARY KEY (clan_id, bm_id)
);
CREATE INDEX idx_clan_members_bm ON bm_clan_members(bm_id);

-- Каждый наблюдённый вайп (из каталога, BattleMetrics или battlemetrics.ru).
CREATE TABLE server_wipes (
    server_id  TEXT NOT NULL,
    at         TEXT NOT NULL,
    source     TEXT NOT NULL,
    PRIMARY KEY (server_id, at)
);
