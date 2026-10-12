-- v2: acoustic fingerprints and storage optimization candidates.

CREATE TABLE copy_fingerprint (
    copy_id     INTEGER NOT NULL REFERENCES book_copy(id) ON DELETE CASCADE,
    segment     INTEGER NOT NULL,
    position_ms INTEGER NOT NULL,     -- position within the whole copy timeline
    length_ms   INTEGER NOT NULL,
    algorithm   TEXT NOT NULL,
    data        BLOB NOT NULL,        -- little-endian u32 sub-fingerprints
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (copy_id, segment)
);

CREATE TABLE optimization_candidate (
    id                   INTEGER PRIMARY KEY,
    copy_id              INTEGER NOT NULL REFERENCES book_copy(id) ON DELETE CASCADE,
    kind                 TEXT NOT NULL,
    target               TEXT NOT NULL,
    current_codec        TEXT NOT NULL,
    current_bitrate_kbps INTEGER,
    target_bitrate_kbps  INTEGER,
    current_size         INTEGER NOT NULL,
    estimated_size       INTEGER NOT NULL,
    reencode             INTEGER NOT NULL,
    preserves_chapters   INTEGER NOT NULL,
    notes                TEXT NOT NULL,
    state                TEXT NOT NULL DEFAULT 'open',   -- open | queued | done | dismissed | failed
    created_at           INTEGER NOT NULL,
    UNIQUE (copy_id, kind, target)
);
CREATE INDEX idx_opt_state ON optimization_candidate(state, current_size);

ALTER TABLE storage_volume ADD COLUMN marker_policy TEXT NOT NULL DEFAULT 'ask';
