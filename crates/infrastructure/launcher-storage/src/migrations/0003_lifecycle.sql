-- What is installed. A row is a receipt: it records what actually happened,
-- including the files the launcher owns, so removal never has to guess.
CREATE TABLE installations (
	id              BLOB    NOT NULL PRIMARY KEY CHECK (length(id) = 16),
	origin          TEXT    NOT NULL CHECK (origin IN ('curated', 'custom')),
	catalog_slug    TEXT,
	display_name    TEXT    NOT NULL,
	repository      TEXT    NOT NULL,
	repository_id   INTEGER NOT NULL,
	owner_id        INTEGER NOT NULL,
	release_id      INTEGER NOT NULL,
	release_tag     TEXT    NOT NULL,
	asset_id        INTEGER NOT NULL,
	asset_name      TEXT    NOT NULL,
	artifact_sha256 TEXT    NOT NULL,
	evidence        TEXT    NOT NULL,
	variant_id      TEXT    NOT NULL,
	adapter         TEXT    NOT NULL,
	format          TEXT    NOT NULL,
	root            TEXT    NOT NULL,
	entry_point     TEXT,
	manifest        TEXT    NOT NULL,
	installed_at    INTEGER NOT NULL,
	updated_at      INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- The durable journal. Memory is never the recovery authority: after a crash
-- the launcher reads this table to decide what to roll back or finish.
CREATE TABLE operations (
	id              BLOB    NOT NULL PRIMARY KEY CHECK (length(id) = 16),
	kind            TEXT    NOT NULL CHECK (kind IN ('install', 'update', 'uninstall')),
	installation_id BLOB,
	plan_digest     BLOB    NOT NULL CHECK (length(plan_digest) = 32),
	plan            TEXT    NOT NULL,
	state           TEXT    NOT NULL,
	staging         TEXT,
	detail          TEXT,
	created_at      INTEGER NOT NULL,
	updated_at      INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX operations_unfinished
	ON operations (state)
	WHERE state NOT IN ('completed', 'failed', 'cancelled');
