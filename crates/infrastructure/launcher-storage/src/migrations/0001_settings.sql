-- Small key/value preferences, such as the theme.
CREATE TABLE settings (
	key   TEXT NOT NULL PRIMARY KEY,
	value TEXT NOT NULL
) STRICT, WITHOUT ROWID;
