-- Game Curator database schema, version 4.
-- Version 2 removed play status, ratings and favourites (Game Curator is an
-- archive, not a backlog). Version 3 added the cover crop and installed games
-- per PC. Version 4 added screenshots and additional files. See db.rs for the
-- upgrades.
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
    thumbnail_path  TEXT,                                 -- Data/Thumbnails/*.webp, rendered from the crop
    cover_crop      TEXT,                                 -- JSON {"x","y","w","h"}, fractions of the cover; NULL = centred
    archive_notes   TEXT    NOT NULL DEFAULT '',          -- "What I know about this game"
    is_available    INTEGER NOT NULL DEFAULT 1 CHECK (is_available IN (0, 1)),
    date_added      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    date_modified   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    last_checked    TEXT
);

CREATE INDEX IF NOT EXISTS idx_games_title_search ON games (title_search);
CREATE INDEX IF NOT EXISTS idx_games_genre        ON games (genre COLLATE NOCASE);
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

-- Where a game is installed. An installation belongs to one computer (the
-- archive drive travels between PCs), so the key includes the computer name.
-- Detected entries ('gog', 'uninstall') are refreshed at every start; entries
-- set by the user ('manual') are only changed by the user.
CREATE TABLE IF NOT EXISTS installations (
    game_id      INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    machine      TEXT    NOT NULL,                        -- computer name, upper case
    install_dir  TEXT    NOT NULL,                        -- absolute path on that computer
    exe_path     TEXT    NOT NULL DEFAULT '',             -- program to start, relative to install_dir ('' = unknown)
    arguments    TEXT    NOT NULL DEFAULT '',
    working_dir  TEXT    NOT NULL DEFAULT '',             -- relative to install_dir ('' = folder of the program)
    source       TEXT    NOT NULL CHECK (source IN ('gog', 'uninstall', 'manual')),
    date_updated TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    PRIMARY KEY (game_id, machine)
) WITHOUT ROWID;

-- Screenshots of a game, taken by the player and added by hand. Stored as
-- optimized WebP copies (the original file is only read). A screenshot can
-- belong to a journal entry; deleting the entry keeps the screenshot.
CREATE TABLE IF NOT EXISTS screenshots (
    id               INTEGER PRIMARY KEY,
    game_id          INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    journal_entry_id INTEGER REFERENCES journal_entries (id) ON DELETE SET NULL,
    image_path       TEXT    NOT NULL,                    -- Data/Screenshots/game-<id>/shot-*.webp
    thumbnail_path   TEXT    NOT NULL,                    -- Data/Thumbnails/shot-*.webp (16:9)
    caption          TEXT    NOT NULL DEFAULT '',
    original_name    TEXT    NOT NULL DEFAULT '',
    width            INTEGER NOT NULL DEFAULT 0,
    height           INTEGER NOT NULL DEFAULT 0,
    taken_at         TEXT,                                -- modification time of the original file
    date_added       TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_screenshots_game ON screenshots (game_id, date_added DESC);
CREATE INDEX IF NOT EXISTS idx_screenshots_entry ON screenshots (journal_entry_id);

-- Additional files of a game: compatibility fixes, community patches, mods,
-- bonus material. Either a copy inside the archive (Data/Additions/game-<id>/)
-- or a link to a file where it already is (relative to the archive root when
-- inside it, otherwise absolute).
CREATE TABLE IF NOT EXISTS additions (
    id          INTEGER PRIMARY KEY,
    game_id     INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL CHECK (kind IN ('copy', 'link')),
    path        TEXT    NOT NULL,
    filename    TEXT    NOT NULL,
    category    TEXT    NOT NULL DEFAULT 'other' CHECK (category IN ('fix', 'patch', 'mod', 'bonus', 'other')),
    note        TEXT    NOT NULL DEFAULT '',
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    date_added  TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_additions_game ON additions (game_id);
