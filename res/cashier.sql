CREATE TABLE IF NOT EXISTS keys(
    key_id INTEGER PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL,
    key_private BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS keypairs(
    dkey_id INTEGER PRIMARY KEY NOT NULL,
    btc_key_private BLOB NOT NULL,
    btc_key_public BLOB NOT NULL,
    txid BLOB NOT NULL
);
