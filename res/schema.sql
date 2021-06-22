PRAGMA key = 'testkey';
CREATE TABLE IF NOT EXISTS keys(
    key_id INTEGER PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL,
    key_private BLOB NOT NULL
);
PRAGMA foreign_keys=on;
CREATE TABLE IF NOT EXISTS coins(
    coin_id INTEGER PRIMARY KEY NOT NULL,
    coin BLOB NOT NULL,
    serial BLOB NOT NULL,
    value INT NOT NULL,
    coin_blind BLOB NOT NULL,
    valcom_blind BLOB NOT NULL,
    tree BLOB NOT NULL,
    filled BLOB NOT NULL,
    cursor_depth BLOB NOT NULL,
    cursor_ BLOB NOT NULL,
    key_id INTEGER NOT NULL,
    FOREIGN KEY (key_id)
        REFERENCES keys (key_id)
);
CREATE TABLE IF NOT EXISTS cashier(
    key_id INTEGER PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL
);
