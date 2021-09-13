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
    coin_blind BLOB NOT NULL,
    valcom_blind BLOB NOT NULL,
    value INT NOT NULL,
    asset_id INT NOT NULL,
    witness BLOB NOT NULL,
    key_id INTEGER NOT NULL,
    FOREIGN KEY (key_id)
        REFERENCES keys (key_id)
        ON UPDATE CASCADE
);
CREATE TABLE IF NOT EXISTS cashier(
    key_id INTEGER PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL
);
