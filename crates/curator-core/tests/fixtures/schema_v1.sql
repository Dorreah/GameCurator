-- Game Curator database schema, version 1.
-- All paths are stored relative to the archive root (forward slashes) unless the
-- target lives on another drive, in which case the absolute path is stored.

CREATE TABLE IF NOT EXISTS games (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    title           TEXT    NOT NULL CHECK (length(trim(title)) > 0),
    title_search    TEXT    NOT NULL DEFAULT '',          -- lowercase, no diacritics
    folder_path     TEXT    NOT NULL,                     -- portable path to the game folder
    folder_key      TEXT    NOT NULL UNIQUE,              -- normalized folder_path, prevents duplicates
    description     TEXT    NOT NULL DEFAULT '',
    developer       TEXT    NOT NULL DEFAULT '',
    publisher       TEXT    NOT NULL DEFAULT '',
    release_year    INTEGER CHECK (release_year IS NULL OR release_year BETWEEN 1940 AND 2100),
    genre           TEXT    NOT NULL DEFAULT '',
    artwork_path    TEXT,                                 -- Data/Artwork/Covers/*.webp
    thumbnail_path  TEXT,                                 -- Data/Thumbnails/*.webp
    rating          INTEGER CHECK (rating IS NULL OR rating BETWEEN 1 AND 5),
    status          TEXT    NOT NULL DEFAULT 'not_played'
                            CHECK (status IN ('not_played', 'playing', 'completed', 'on_hold')),
    is_favorite     INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
    archive_notes   TEXT    NOT NULL DEFAULT '',
    is_available    INTEGER NOT NULL DEFAULT 1 CHECK (is_available IN (0, 1)),
    date_added      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    date_modified   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    last_checked    TEXT
);

CREATE INDEX IF NOT EXISTS idx_games_title_search ON games (title_search);
CREATE INDEX IF NOT EXISTS idx_games_status       ON games (status);
CREATE INDEX IF NOT EXISTS idx_games_genre        ON games (genre COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_games_favorite     ON games (is_favorite) WHERE is_favorite = 1;
CREATE INDEX IF NOT EXISTS idx_games_date_added   ON games (date_added);

CREATE TABLE IF NOT EXISTS files (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id         INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    file_path       TEXT    NOT NULL,                     -- relative to the game folder
    filename        TEXT    NOT NULL,
    file_type       TEXT    NOT NULL,                     -- auto-detected category
    custom_category TEXT,                                 -- user override (NULL = use file_type)
    is_primary      INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    size_bytes      INTEGER,
    is_missing      INTEGER NOT NULL DEFAULT 0 CHECK (is_missing IN (0, 1)),
    last_seen       TEXT,
    UNIQUE (game_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_files_game ON files (game_id);
-- At most one primary installer per game.
CREATE UNIQUE INDEX IF NOT EXISTS idx_files_one_primary ON files (game_id) WHERE is_primary = 1;

CREATE TABLE IF NOT EXISTS journal_entries (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id         INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    entry_date      TEXT    NOT NULL,                     -- YYYY-MM-DD, editable by the user
    title           TEXT    NOT NULL DEFAULT '',
    content         TEXT    NOT NULL DEFAULT '',          -- sanitized HTML (p, h2, h3, ul, ol, li, b, i, u, blockquote)
    rating          INTEGER CHECK (rating IS NULL OR rating BETWEEN 1 AND 5),
    date_created    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    date_modified   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_journal_game_date ON journal_entries (game_id, entry_date DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_journal_date      ON journal_entries (entry_date DESC, id DESC);

CREATE TABLE IF NOT EXISTS tags (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE IF NOT EXISTS game_tags (
    game_id         INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    tag_id          INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (game_id, tag_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_game_tags_tag ON game_tags (tag_id);

CREATE TABLE IF NOT EXISTS settings (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    setting_key     TEXT    NOT NULL UNIQUE,
    setting_value   TEXT
);
