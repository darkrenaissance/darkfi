CREATE TABLE IF NOT EXISTS channels (
    name   TEXT PRIMARY KEY,
    secret BLOB
);
CREATE TABLE IF NOT EXISTS contacts (
    name   TEXT PRIMARY KEY,
    public BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS profiles (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    nick      TEXT NOT NULL,
    dm_secret BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    name  TEXT NOT NULL,
    idx   INTEGER NOT NULL,
    type  TEXT NOT NULL,
    value BLOB NOT NULL,
    PRIMARY KEY (name, idx)
);
CREATE TABLE IF NOT EXISTS flags (
    name  TEXT PRIMARY KEY,
    value BLOB NOT NULL
);
