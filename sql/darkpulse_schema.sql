
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS node (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	username TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS channel(
	channel_id INTEGER PRIMARY KEY AUTOINCREMENT,
	channel_name TEXT NOT NULL,
	channel_secret INT(32) NOT NULL,
	address CHAR(58) NOT NULL,
	UNIQUE("channel_name")
);

CREATE TABLE IF NOT EXISTS slab(
	slab_id INTEGER PRIMARY KEY AUTOINCREMENT,
	nonce INT(12)  NOT NULL,
	cipher_text BLOB NOT NULL ,
	cipher_text_hash INT(32) NOT NULL,
	channel_id INTEGER NOT NULL,
	FOREIGN KEY(channel_id) REFERENCES channel(channel_id)
	UNIQUE("cipher_text_hash")
);

COMMIT;


