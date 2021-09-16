CREATE TABLE IF NOT EXISTS deposit_keypairs(
    d_key_public INTEGER PRIMARY KEY NOT NULL,
   	coin_key_private BLOB NOT NULL,
    coin_key_public BLOB NOT NULL,
	asset_id BLOB NOT NULL
);
PRAGMA foreign_keys=on;
CREATE TABLE IF NOT EXISTS btc_utxo(
    tx_id BLOB PRIMARY KEY NOT NULL,
    balance INTEGER NOT NULL,
    btc_key_public BLOB NOT NULL,
    FOREIGN KEY (btc_key_public)
        REFERENCES deposit_keypairs (coin_key_public)
        ON UPDATE CASCADE
);
CREATE TABLE IF NOT EXISTS withdraw_keypairs(
    coin_key_id BLOB PRIMARY KEY NOT NULL,
	d_key_private BLOB NOT NULL,
    d_key_public BLOB NOT NULL,
	asset_id BLOB NOT NULL,
	confirm BLOB NOT NULL
);
