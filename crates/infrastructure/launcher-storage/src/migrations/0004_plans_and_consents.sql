-- Sealed plans, kept so `plan` and `apply` can be separate commands (and so
-- a GUI can review a plan a CLI made). A plan row is immutable: its digest
-- covers the body.
CREATE TABLE plans (
	id         TEXT    NOT NULL PRIMARY KEY,
	digest     BLOB    NOT NULL UNIQUE CHECK (length(digest) = 32),
	body       TEXT    NOT NULL,
	extra      TEXT    NOT NULL,
	update_of  BLOB,
	created_at INTEGER NOT NULL,
	expires_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- What a person agreed to, who or what recorded it, and when. A consent is
-- evidence of a decision, never a substitute for verifying an artifact.
CREATE TABLE consents (
	id             INTEGER NOT NULL PRIMARY KEY,
	kind           TEXT    NOT NULL CHECK (kind IN ('plan', 'update-mode', 'unattended-source')),
	subject        TEXT    NOT NULL,
	scope          TEXT    NOT NULL,
	policy_version INTEGER NOT NULL,
	actor          TEXT    NOT NULL,
	granted_at     INTEGER NOT NULL,
	revoked_at     INTEGER
) STRICT;

CREATE INDEX consents_lookup ON consents (kind, subject) WHERE revoked_at IS NULL;
