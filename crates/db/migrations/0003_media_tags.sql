-- v3: keep audiobook-specific tag fields per media file so re-analysis does
-- not need to re-read files that did not change.

ALTER TABLE media_file ADD COLUMN tag_narrator TEXT;
ALTER TABLE media_file ADD COLUMN tag_series TEXT;
ALTER TABLE media_file ADD COLUMN tag_series_index TEXT;
ALTER TABLE media_file ADD COLUMN tag_description TEXT;
ALTER TABLE media_file ADD COLUMN tag_language TEXT;
ALTER TABLE media_file ADD COLUMN tag_publisher TEXT;
ALTER TABLE media_file ADD COLUMN container TEXT;
