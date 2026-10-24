-- Apps that were already on the computer and that the person asked the
-- launcher to keep an eye on. They are observations, not installs: the
-- launcher did not put these files there, so it never removes or replaces
-- them. A row can be dropped at any time without touching a single file.
CREATE TABLE adopted_apps (
	id            BLOB    NOT NULL PRIMARY KEY CHECK (length(id) = 16),
	display_name  TEXT    NOT NULL,
	path          TEXT    NOT NULL UNIQUE,
	entry_point   TEXT,
	-- What the person said this is. It is a claim, never proof, and nothing
	-- is updated or removed because of it.
	catalog_slug  TEXT,
	adopted_at    INTEGER NOT NULL,
	last_seen_at  INTEGER
) STRICT, WITHOUT ROWID;
