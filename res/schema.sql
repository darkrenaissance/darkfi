PRAGMA key = 'testkey';
CREATE TABLE IF NOT EXISTS keys(
    key_id INTEGER PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL,
    key_private BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS coins(
    coin_id INTEGER PRIMARY KEY NOT NULL,
    coin BLOB NOT NULL,
    witness BLOB NOT NULL,
    serial BLOB NOT NULL,
    value INT NOT NULL,
    coin_blind BLOB NOT NULL,
    valcom_blind BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS cashier(
    key_id INTEGER PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL
);
