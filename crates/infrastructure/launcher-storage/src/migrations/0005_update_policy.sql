-- Per-app automatic-update preferences. Preferences can only delay or pause
-- an update. They are never a trust root.
CREATE TABLE app_update_policy (
	installation_id   BLOB    NOT NULL PRIMARY KEY CHECK (length(installation_id) = 16),
	min_age_seconds   INTEGER,
	auto_paused       INTEGER NOT NULL DEFAULT 0 CHECK (auto_paused IN (0, 1)),
	pinned_release_id INTEGER,
	allow_prerelease  INTEGER NOT NULL DEFAULT 0 CHECK (allow_prerelease IN (0, 1)),
	unattended_custom INTEGER NOT NULL DEFAULT 0 CHECK (unattended_custom IN (0, 1))
) STRICT, WITHOUT ROWID;

-- Every release asset the launcher has seen, with when it was last known to
-- change. `window_start` is where the minimum-age clock starts: it moves
-- forward when an asset's identity changes.
CREATE TABLE asset_observations (
	repository_id        INTEGER NOT NULL,
	release_id           INTEGER NOT NULL,
	asset_id             INTEGER NOT NULL,
	tag                  TEXT    NOT NULL,
	fingerprint          TEXT    NOT NULL,
	size                 INTEGER NOT NULL,
	host_sha256          TEXT,
	release_published_at INTEGER,
	asset_created_at     INTEGER,
	asset_updated_at     INTEGER,
	first_seen_at        INTEGER NOT NULL,
	last_seen_at         INTEGER NOT NULL,
	window_start         INTEGER NOT NULL,
	PRIMARY KEY (repository_id, release_id, asset_id)
) STRICT, WITHOUT ROWID;

-- Why each automatic update was or was not applied.
CREATE TABLE update_decisions (
	id                   INTEGER NOT NULL PRIMARY KEY,
	installation_id      BLOB    NOT NULL,
	cycle_id             TEXT    NOT NULL,
	trigger_kind         TEXT    NOT NULL,
	candidate_release_id INTEGER,
	candidate_asset_id   INTEGER,
	outcome              TEXT    NOT NULL,
	reason               TEXT    NOT NULL,
	policy               TEXT    NOT NULL,
	plan_digest          BLOB,
	decided_at           INTEGER NOT NULL
) STRICT;

CREATE INDEX update_decisions_by_app ON update_decisions (installation_id, decided_at DESC);
