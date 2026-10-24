-- The verified catalog and its trust high-water mark, in one row per feed so
-- they can never disagree. Ordinary cache clearing must not touch this table.
CREATE TABLE catalog_trust (
	feed_id      TEXT    NOT NULL PRIMARY KEY,
	revision     INTEGER NOT NULL CHECK (revision >= 1),
	digest       BLOB    NOT NULL CHECK (length(digest) = 32),
	manifest     BLOB    NOT NULL,
	envelope     BLOB    NOT NULL,
	accepted_at  INTEGER NOT NULL,
	last_seen_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
