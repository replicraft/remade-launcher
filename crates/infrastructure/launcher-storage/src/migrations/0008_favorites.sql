-- Apps the person has marked to come back to. Only references: a catalog
-- name or an owner/repository pair. Nothing here installs anything or
-- vouches for anything.
CREATE TABLE favorites (
	kind       TEXT    NOT NULL CHECK (kind IN ('catalog', 'repository')),
	reference  TEXT    NOT NULL,
	added_at   INTEGER NOT NULL,
	PRIMARY KEY (kind, reference)
) STRICT, WITHOUT ROWID;
