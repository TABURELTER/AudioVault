-- AudioVault reading status and book deletion queue / history v5.

ALTER TABLE book ADD COLUMN is_listened INTEGER NOT NULL DEFAULT 0;
ALTER TABLE book ADD COLUMN is_marked_for_deletion INTEGER NOT NULL DEFAULT 0;

ALTER TABLE ebook_entry ADD COLUMN is_read INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ebook_entry ADD COLUMN is_marked_for_deletion INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_book_is_listened ON book(is_listened);
CREATE INDEX idx_book_marked_del ON book(is_marked_for_deletion);

CREATE INDEX idx_ebook_is_read ON ebook_entry(is_read);
CREATE INDEX idx_ebook_marked_del ON ebook_entry(is_marked_for_deletion);

-- Pending deletion queue for offline storage volumes
CREATE TABLE pending_deletion (
    id             INTEGER PRIMARY KEY,
    volume_id      INTEGER NOT NULL REFERENCES storage_volume(id) ON DELETE CASCADE,
    item_type      TEXT NOT NULL,
    item_id        INTEGER NOT NULL,
    rel_path       TEXT NOT NULL,
    title          TEXT NOT NULL,
    authors        TEXT NOT NULL DEFAULT '',
    size           INTEGER NOT NULL DEFAULT 0,
    status         TEXT NOT NULL DEFAULT 'pending',
    created_at     INTEGER NOT NULL,
    executed_at    INTEGER,
    error_message  TEXT
);
CREATE INDEX idx_pending_del_volume ON pending_deletion(volume_id, status);

-- Audit history for all deleted books
CREATE TABLE deletion_history (
    id             INTEGER PRIMARY KEY,
    item_type      TEXT NOT NULL,
    title          TEXT NOT NULL,
    authors        TEXT NOT NULL DEFAULT '',
    narrator       TEXT NOT NULL DEFAULT '',
    format         TEXT NOT NULL DEFAULT '',
    total_size     INTEGER NOT NULL DEFAULT 0,
    duration_ms    INTEGER NOT NULL DEFAULT 0,
    volume_name    TEXT NOT NULL DEFAULT '',
    mount_path     TEXT NOT NULL DEFAULT '',
    rel_path       TEXT NOT NULL DEFAULT '',
    status         TEXT NOT NULL DEFAULT 'deleted',
    deleted_at     INTEGER NOT NULL
);
CREATE INDEX idx_deletion_history_time ON deletion_history(deleted_at DESC);
