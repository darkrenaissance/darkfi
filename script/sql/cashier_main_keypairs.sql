CREATE TABLE IF NOT EXISTS main_keypairs(
	keypair_id INTEGER PRIMARY KEY NOT NULL,
	token_key_secret BLOB NOT NULL,
	token_key_public BLOB NOT NULL,
	network BLOB NOT NULL
);
