-- AudioVault e-books catalog schema v4.
-- Independent catalog for electronic books (FB2, EPUB, PDF, DJVU, MOBI, etc.)
-- Links to storage_volume so offline/online volume tracking works identically.

CREATE TABLE ebook_entry (
    id             INTEGER PRIMARY KEY,
    volume_id      INTEGER NOT NULL REFERENCES storage_volume(id) ON DELETE CASCADE,
    root_id        INTEGER REFERENCES library_root(id) ON DELETE CASCADE,
    rel_path       TEXT NOT NULL,
    file_name      TEXT NOT NULL,
    format         TEXT NOT NULL,
    size           INTEGER NOT NULL,
    mtime          INTEGER NOT NULL,
    title          TEXT NOT NULL,
    authors        TEXT NOT NULL DEFAULT '',
    series         TEXT,
    series_index   REAL,
    genres         TEXT,
    year           INTEGER,
    language       TEXT,
    description    TEXT,
    cover_thumb    TEXT,
    state          TEXT NOT NULL DEFAULT 'present',
    last_scan_id   INTEGER,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    UNIQUE(volume_id, rel_path)
);

CREATE INDEX idx_ebook_volume_state ON ebook_entry(volume_id, state);
CREATE INDEX idx_ebook_authors ON ebook_entry(authors);
CREATE INDEX idx_ebook_series ON ebook_entry(series);
CREATE INDEX idx_ebook_format ON ebook_entry(format);
CREATE INDEX idx_ebook_title ON ebook_entry(title);

-- Full-text search for e-books. rowid = ebook_entry.id
CREATE VIRTUAL TABLE ebook_fts USING fts5(
    title, authors, series, genres, description,
    tokenize = "unicode61 remove_diacritics 0",
    prefix = '2 3'
);

-- Triggers to keep ebook_fts in sync with ebook_entry
CREATE TRIGGER ebook_ai AFTER INSERT ON ebook_entry BEGIN
    INSERT INTO ebook_fts(rowid, title, authors, series, genres, description)
    VALUES (new.id, new.title, new.authors, coalesce(new.series, ''), coalesce(new.genres, ''), coalesce(new.description, ''));
END;

CREATE TRIGGER ebook_ad AFTER DELETE ON ebook_entry BEGIN
    DELETE FROM ebook_fts WHERE rowid = old.id;
END;

CREATE TRIGGER ebook_au AFTER UPDATE ON ebook_entry BEGIN
    DELETE FROM ebook_fts WHERE rowid = old.id;
    INSERT INTO ebook_fts(rowid, title, authors, series, genres, description)
    VALUES (new.id, new.title, new.authors, coalesce(new.series, ''), coalesce(new.genres, ''), coalesce(new.description, ''));
END;
