CREATE TABLE IF NOT EXISTS main_keypairs(
    keypair_id INTEGER PRIMARY KEY NOT NULL,
   	token_key_private BLOB NOT NULL,
    token_key_public BLOB NOT NULL,
	network BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS deposit_keypairs(
    keypair_id INTEGER PRIMARY KEY NOT NULL,
    d_key_public BLOB NOT NULL,
   	token_key_private BLOB NOT NULL,
    token_key_public BLOB NOT NULL,
	network BLOB NOT NULL,
	asset_id BLOB NOT NULL,
	confirm BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS withdraw_keypairs(
    keypair_id INTEGER PRIMARY KEY NOT NULL,
    token_key_public BLOB NOT NULL,
	d_key_private BLOB NOT NULL,
    d_key_public BLOB NOT NULL,
	network BLOB NOT NULL,
	asset_id BLOB NOT NULL,
	confirm BLOB NOT NULL
);
