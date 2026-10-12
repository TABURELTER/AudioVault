-- AudioVault catalog schema v1.
-- Paths of catalog objects are stored RELATIVE to their storage volume's
-- mount root with '/' separators, so drive-letter / mount-point changes never
-- invalidate the catalog. Timestamps are Unix seconds (UTC).

CREATE TABLE app_setting (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE storage_volume (
    id               INTEGER PRIMARY KEY,
    uuid             TEXT NOT NULL UNIQUE,          -- internal application identity
    display_name     TEXT NOT NULL,
    volume_label     TEXT,
    fs_type          TEXT,
    capacity_bytes   INTEGER,
    free_bytes       INTEGER,
    platform_id      TEXT,                           -- Windows Volume GUID path / macOS volume UUID
    serial           TEXT,                           -- secondary identity (volume serial number)
    marker_uuid      TEXT UNIQUE,                    -- .audiovault-id marker content (optional)
    last_mount_path  TEXT,
    storage_class    TEXT NOT NULL DEFAULT 'unknown',
    is_online        INTEGER NOT NULL DEFAULT 0,
    first_indexed_at INTEGER,
    last_seen_at     INTEGER,
    last_scanned_at  INTEGER,
    scan_settings    TEXT NOT NULL DEFAULT '{}',
    last_error       TEXT,
    created_at       INTEGER NOT NULL
);
CREATE INDEX idx_volume_platform ON storage_volume(platform_id);
CREATE INDEX idx_volume_serial ON storage_volume(serial);

-- A folder on a volume that the user asked to index (may be the volume root: rel_path = '').
CREATE TABLE library_root (
    id            INTEGER PRIMARY KEY,
    volume_id     INTEGER NOT NULL REFERENCES storage_volume(id) ON DELETE CASCADE,
    rel_path      TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    last_scan_at  INTEGER,
    created_at    INTEGER NOT NULL,
    UNIQUE (volume_id, rel_path)
);

CREATE TABLE person (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,           -- canonical display
    sort_name   TEXT NOT NULL,
    person_key  TEXT NOT NULL,           -- surname-skeleton|initial
    surname_key TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_person_key ON person(person_key);
CREATE INDEX idx_person_surname ON person(surname_key);

CREATE TABLE person_alias (
    id         INTEGER PRIMARY KEY,
    person_id  INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    alias      TEXT NOT NULL,
    norm       TEXT NOT NULL,
    source     TEXT NOT NULL,
    UNIQUE (person_id, norm)
);

CREATE TABLE series (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    series_key  TEXT NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL
);

CREATE TABLE book (
    id                  INTEGER PRIMARY KEY,
    uuid                TEXT NOT NULL UNIQUE,
    title               TEXT NOT NULL,
    sort_title          TEXT NOT NULL,
    title_key           TEXT NOT NULL,
    identity_key        TEXT NOT NULL,
    authors_display     TEXT NOT NULL DEFAULT '',
    sort_author         TEXT NOT NULL DEFAULT '',
    authors_key         TEXT NOT NULL DEFAULT '',
    series_id           INTEGER REFERENCES series(id) ON DELETE SET NULL,
    series_name         TEXT,
    series_index        REAL,
    year                INTEGER,
    language            TEXT,
    genre               TEXT,
    description         TEXT,
    publisher           TEXT,
    narrators_display   TEXT NOT NULL DEFAULT '',
    preferred_variant_id INTEGER,
    preferred_copy_id   INTEGER,
    variant_count       INTEGER NOT NULL DEFAULT 0,
    copy_count          INTEGER NOT NULL DEFAULT 0,
    present_copy_count  INTEGER NOT NULL DEFAULT 0,
    total_duration_ms   INTEGER NOT NULL DEFAULT 0,
    total_size          INTEGER NOT NULL DEFAULT 0,
    format              TEXT NOT NULL DEFAULT '',
    has_fb2             INTEGER NOT NULL DEFAULT 0,
    has_duplicates      INTEGER NOT NULL DEFAULT 0,
    issue_count         INTEGER NOT NULL DEFAULT 0,
    cover_thumb         TEXT,
    metadata_confidence REAL NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);
CREATE INDEX idx_book_identity ON book(identity_key);
CREATE INDEX idx_book_title_key ON book(title_key);
CREATE INDEX idx_book_sort_title ON book(sort_title);
CREATE INDEX idx_book_sort_author ON book(sort_author, sort_title);
CREATE INDEX idx_book_created ON book(created_at);
CREATE INDEX idx_book_size ON book(total_size);
CREATE INDEX idx_book_duration ON book(total_duration_ms);
CREATE INDEX idx_book_dups ON book(has_duplicates) WHERE has_duplicates = 1;
CREATE INDEX idx_book_issues ON book(issue_count) WHERE issue_count > 0;

CREATE TABLE book_alias (
    id       INTEGER PRIMARY KEY,
    book_id  INTEGER NOT NULL REFERENCES book(id) ON DELETE CASCADE,
    alias    TEXT NOT NULL,
    norm     TEXT NOT NULL,
    kind     TEXT NOT NULL,           -- title | translit | reverse_translit | original
    source   TEXT NOT NULL,
    UNIQUE (book_id, norm)
);

CREATE TABLE book_author (
    book_id   INTEGER NOT NULL REFERENCES book(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    role      TEXT NOT NULL DEFAULT 'author',
    position  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (book_id, person_id, role)
);
CREATE INDEX idx_book_author_person ON book_author(person_id);

CREATE TABLE audio_variant (
    id                INTEGER PRIMARY KEY,
    book_id           INTEGER NOT NULL REFERENCES book(id) ON DELETE CASCADE,
    narrator_display  TEXT NOT NULL DEFAULT '',
    narrator_key      TEXT NOT NULL DEFAULT '',
    label             TEXT,
    preferred_copy_id INTEGER,
    created_at        INTEGER NOT NULL
);
CREATE INDEX idx_variant_book ON audio_variant(book_id);

CREATE TABLE variant_narrator (
    variant_id INTEGER NOT NULL REFERENCES audio_variant(id) ON DELETE CASCADE,
    person_id  INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (variant_id, person_id)
);

CREATE TABLE book_copy (
    id                   INTEGER PRIMARY KEY,
    uuid                 TEXT NOT NULL UNIQUE,
    variant_id           INTEGER NOT NULL REFERENCES audio_variant(id) ON DELETE CASCADE,
    book_id              INTEGER NOT NULL REFERENCES book(id) ON DELETE CASCADE,
    volume_id            INTEGER NOT NULL REFERENCES storage_volume(id) ON DELETE CASCADE,
    root_id              INTEGER REFERENCES library_root(id) ON DELETE SET NULL,
    rel_path             TEXT NOT NULL,               -- media-set folder (or single file) relative to volume
    is_single_file       INTEGER NOT NULL DEFAULT 0,
    state                TEXT NOT NULL DEFAULT 'present',
    file_count           INTEGER NOT NULL DEFAULT 0,
    audio_file_count     INTEGER NOT NULL DEFAULT 0,
    total_size           INTEGER NOT NULL DEFAULT 0,
    audio_size           INTEGER NOT NULL DEFAULT 0,
    total_duration_ms    INTEGER NOT NULL DEFAULT 0,
    primary_codec        TEXT,
    container            TEXT,
    avg_bitrate_kbps     INTEGER,
    sample_rate          INTEGER,
    channels             INTEGER,
    is_lossless          INTEGER NOT NULL DEFAULT 0,
    has_cover            INTEGER NOT NULL DEFAULT 0,
    has_fb2              INTEGER NOT NULL DEFAULT 0,
    has_chapters         INTEGER NOT NULL DEFAULT 0,
    probe_errors         INTEGER NOT NULL DEFAULT 0,
    detection_confidence REAL NOT NULL DEFAULT 0,
    detection_evidence   TEXT NOT NULL DEFAULT '[]',
    quick_signature      TEXT,                         -- digest of sorted (name,size) of audio files
    analysis_level       INTEGER NOT NULL DEFAULT 0,   -- 0 enumerated, 1 metadata, 2 quick-hashed, 3 full-hashed, 4 fingerprinted
    quality_score        REAL,
    narrator_hint        TEXT,
    first_seen_at        INTEGER NOT NULL,
    last_seen_at         INTEGER NOT NULL,
    missing_since        INTEGER,
    last_scan_id         INTEGER,
    UNIQUE (volume_id, rel_path)
);
CREATE INDEX idx_copy_book ON book_copy(book_id);
CREATE INDEX idx_copy_variant ON book_copy(variant_id);
CREATE INDEX idx_copy_volume_state ON book_copy(volume_id, state);
CREATE INDEX idx_copy_signature ON book_copy(quick_signature);
CREATE INDEX idx_copy_analysis ON book_copy(analysis_level);

CREATE TABLE media_file (
    id             INTEGER PRIMARY KEY,
    copy_id        INTEGER NOT NULL REFERENCES book_copy(id) ON DELETE CASCADE,
    volume_id      INTEGER NOT NULL REFERENCES storage_volume(id) ON DELETE CASCADE,
    rel_path       TEXT NOT NULL,
    file_name      TEXT NOT NULL,
    ext            TEXT NOT NULL,
    kind           TEXT NOT NULL,
    size           INTEGER NOT NULL,
    mtime          INTEGER NOT NULL,
    sort_index     INTEGER NOT NULL DEFAULT 0,
    duration_ms    INTEGER,
    codec          TEXT,
    bitrate_kbps   INTEGER,
    sample_rate    INTEGER,
    channels       INTEGER,
    track_no       INTEGER,
    disc_no        INTEGER,
    tag_title      TEXT,
    tag_artist     TEXT,
    tag_album      TEXT,
    tag_album_artist TEXT,
    tag_composer   TEXT,
    tag_genre      TEXT,
    tag_year       INTEGER,
    tag_comment    TEXT,
    has_embedded_cover INTEGER NOT NULL DEFAULT 0,
    chapter_count  INTEGER NOT NULL DEFAULT 0,
    probed_at      INTEGER,
    probe_error    TEXT,
    quick_hash     TEXT,
    full_hash      TEXT,
    hashed_size    INTEGER,
    hashed_mtime   INTEGER,
    state          TEXT NOT NULL DEFAULT 'present',
    last_scan_id   INTEGER,
    UNIQUE (volume_id, rel_path)
);
CREATE INDEX idx_media_copy ON media_file(copy_id, sort_index);
CREATE INDEX idx_media_full_hash ON media_file(full_hash) WHERE full_hash IS NOT NULL;
CREATE INDEX idx_media_quick_hash ON media_file(quick_hash) WHERE quick_hash IS NOT NULL;

CREATE TABLE ebook_file (
    id             INTEGER PRIMARY KEY,
    copy_id        INTEGER NOT NULL REFERENCES book_copy(id) ON DELETE CASCADE,
    media_file_id  INTEGER NOT NULL UNIQUE REFERENCES media_file(id) ON DELETE CASCADE,
    format         TEXT NOT NULL,
    title          TEXT,
    authors        TEXT,
    series         TEXT,
    series_index   REAL,
    genres         TEXT,
    annotation     TEXT,
    language       TEXT,
    publisher      TEXT,
    year           INTEGER,
    document_id    TEXT,
    has_cover      INTEGER NOT NULL DEFAULT 0,
    match_score    REAL,
    parsed_at      INTEGER NOT NULL,
    error          TEXT
);
CREATE INDEX idx_ebook_copy ON ebook_file(copy_id);

CREATE TABLE cover (
    id           INTEGER PRIMARY KEY,
    book_id      INTEGER REFERENCES book(id) ON DELETE CASCADE,
    copy_id      INTEGER REFERENCES book_copy(id) ON DELETE CASCADE,
    source       TEXT NOT NULL,          -- folder | embedded | fb2 | manual
    source_ref   TEXT,                   -- rel path of image or audio file
    width        INTEGER,
    height       INTEGER,
    content_hash TEXT,
    thumb_path   TEXT,                   -- file in local cover cache
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_cover_book ON cover(book_id);
CREATE INDEX idx_cover_copy ON cover(copy_id);

CREATE TABLE metadata_observation (
    id          INTEGER PRIMARY KEY,
    book_id     INTEGER NOT NULL REFERENCES book(id) ON DELETE CASCADE,
    copy_id     INTEGER REFERENCES book_copy(id) ON DELETE CASCADE,
    field       TEXT NOT NULL,
    value       TEXT NOT NULL,
    source      TEXT NOT NULL,
    confidence  REAL NOT NULL,
    observed_at INTEGER NOT NULL
);
CREATE INDEX idx_obs_book ON metadata_observation(book_id, field);
CREATE INDEX idx_obs_copy ON metadata_observation(copy_id);

CREATE TABLE metadata_issue (
    id              INTEGER PRIMARY KEY,
    book_id         INTEGER NOT NULL REFERENCES book(id) ON DELETE CASCADE,
    copy_id         INTEGER REFERENCES book_copy(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,
    message         TEXT NOT NULL,
    field           TEXT,
    current_value   TEXT,
    proposed_value  TEXT,
    state           TEXT NOT NULL DEFAULT 'open',   -- open | applied | dismissed
    created_at      INTEGER NOT NULL,
    UNIQUE (book_id, kind, field)
);
CREATE INDEX idx_issue_state ON metadata_issue(state);

CREATE TABLE copy_block_key (
    key      TEXT NOT NULL,
    copy_id  INTEGER NOT NULL REFERENCES book_copy(id) ON DELETE CASCADE,
    PRIMARY KEY (key, copy_id)
) WITHOUT ROWID;
CREATE INDEX idx_block_copy ON copy_block_key(copy_id);

CREATE TABLE duplicate_candidate (
    id               INTEGER PRIMARY KEY,
    copy_a           INTEGER NOT NULL REFERENCES book_copy(id) ON DELETE CASCADE,
    copy_b           INTEGER NOT NULL REFERENCES book_copy(id) ON DELETE CASCADE,
    score            REAL NOT NULL,
    verdict          TEXT NOT NULL,
    evidence         TEXT NOT NULL,
    acoustic_checked INTEGER NOT NULL DEFAULT 0,
    recommended_keep INTEGER,
    status           TEXT NOT NULL DEFAULT 'open',
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE (copy_a, copy_b),
    CHECK (copy_a < copy_b)
);
CREATE INDEX idx_dup_status ON duplicate_candidate(status, score);
CREATE INDEX idx_dup_b ON duplicate_candidate(copy_b);

CREATE TABLE torrent_transfer (
    id              INTEGER PRIMARY KEY,
    info_hash       TEXT,
    name            TEXT NOT NULL,
    source_kind     TEXT NOT NULL,       -- magnet | file | provider
    magnet          TEXT,
    torrent_data    BLOB,
    provider        TEXT,
    provider_ref    TEXT,
    volume_id       INTEGER REFERENCES storage_volume(id) ON DELETE SET NULL,
    dest_rel_path   TEXT NOT NULL,       -- relative to volume root
    selected_files  TEXT,                -- JSON array of file indices or NULL = all
    state           TEXT NOT NULL,
    progress        REAL NOT NULL DEFAULT 0,
    total_bytes     INTEGER NOT NULL DEFAULT 0,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    uploaded_bytes  INTEGER NOT NULL DEFAULT 0,
    engine_id       INTEGER,
    error           TEXT,
    created_at      INTEGER NOT NULL,
    completed_at    INTEGER,
    imported_at     INTEGER
);
CREATE INDEX idx_torrent_state ON torrent_transfer(state);

CREATE TABLE background_job (
    id               INTEGER PRIMARY KEY,
    kind             TEXT NOT NULL,
    state            TEXT NOT NULL,
    priority         INTEGER NOT NULL DEFAULT 0,
    volume_id        INTEGER REFERENCES storage_volume(id) ON DELETE CASCADE,
    payload          TEXT NOT NULL DEFAULT '{}',
    title            TEXT NOT NULL DEFAULT '',
    progress         REAL NOT NULL DEFAULT 0,
    progress_text    TEXT NOT NULL DEFAULT '',
    error            TEXT,
    attempts         INTEGER NOT NULL DEFAULT 0,
    max_attempts     INTEGER NOT NULL DEFAULT 3,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    dedupe_key       TEXT,
    created_at       INTEGER NOT NULL,
    started_at       INTEGER,
    finished_at      INTEGER,
    updated_at       INTEGER NOT NULL
);
CREATE INDEX idx_job_state ON background_job(state, priority DESC, id);
CREATE INDEX idx_job_dedupe ON background_job(dedupe_key, state);

CREATE TABLE file_operation (
    id            INTEGER PRIMARY KEY,
    kind          TEXT NOT NULL,
    state         TEXT NOT NULL,
    volume_id     INTEGER REFERENCES storage_volume(id) ON DELETE SET NULL,
    dest_volume_id INTEGER REFERENCES storage_volume(id) ON DELETE SET NULL,
    copy_id       INTEGER REFERENCES book_copy(id) ON DELETE SET NULL,
    source_path   TEXT NOT NULL,          -- absolute path at time of planning
    dest_path     TEXT,
    payload       TEXT NOT NULL DEFAULT '{}',
    undo          TEXT,
    confirmed     INTEGER NOT NULL DEFAULT 0,
    error         TEXT,
    created_at    INTEGER NOT NULL,
    started_at    INTEGER,
    finished_at   INTEGER
);
CREATE INDEX idx_fileop_state ON file_operation(state);

CREATE TABLE scan_run (
    id           INTEGER PRIMARY KEY,
    volume_id    INTEGER NOT NULL REFERENCES storage_volume(id) ON DELETE CASCADE,
    root_id      INTEGER REFERENCES library_root(id) ON DELETE CASCADE,
    state        TEXT NOT NULL,          -- running | completed | interrupted | failed
    started_at   INTEGER NOT NULL,
    finished_at  INTEGER,
    dirs_seen    INTEGER NOT NULL DEFAULT 0,
    files_seen   INTEGER NOT NULL DEFAULT 0,
    copies_found INTEGER NOT NULL DEFAULT 0,
    copies_new   INTEGER NOT NULL DEFAULT 0,
    copies_missing INTEGER NOT NULL DEFAULT 0,
    errors       INTEGER NOT NULL DEFAULT 0,
    message      TEXT
);

CREATE TABLE scan_error (
    id        INTEGER PRIMARY KEY,
    scan_id   INTEGER NOT NULL REFERENCES scan_run(id) ON DELETE CASCADE,
    rel_path  TEXT NOT NULL,
    message   TEXT NOT NULL,
    at        INTEGER NOT NULL
);
CREATE INDEX idx_scan_error_scan ON scan_error(scan_id);

-- Full-text search. rowid = book.id. Maintained explicitly by the catalog layer.
CREATE VIRTUAL TABLE book_fts USING fts5(
    title, authors, series, narrators, aliases, skel,
    tokenize = "unicode61 remove_diacritics 0",
    prefix = '2 3'
);
CREATE VIRTUAL TABLE book_fts_vocab USING fts5vocab(book_fts, 'row');
