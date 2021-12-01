CREATE TABLE IF NOT EXISTS coins(
	coin BLOB PRIMARY KEY NOT NULL,
	serial BLOB NOT NULL,
	coin_blind BLOB NOT NULL,
	valcom_blind BLOB NOT NULL,
	value BLOB NOT NULL,
	token_id BLOB NOT NULL,
	secret BLOB NOT NULL,
	is_spent BOOLEAN NOT NULL,
	nullifier BLOB NOT NULL
);
