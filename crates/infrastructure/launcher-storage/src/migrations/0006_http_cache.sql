-- Remembered answers from GitHub's API, with the validator GitHub gave. The
-- next lookup sends it back, and a "not modified" answer costs nothing
-- against the rate limit. Everything here can be thrown away at any time.
CREATE TABLE http_cache (
	url        TEXT    NOT NULL PRIMARY KEY,
	etag       TEXT    NOT NULL,
	body       BLOB    NOT NULL,
	stored_at  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX http_cache_age ON http_cache (stored_at);
