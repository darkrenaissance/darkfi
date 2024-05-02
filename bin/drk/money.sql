-- Wallet definitions for this contract.
-- We store data that is needed to be able to receive and send tokens.

-- Arbitrary info that is potentially useful
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_info (
	last_scanned_block INTEGER NOT NULL
);

-- The Merkle tree containing coins
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_tree (
	tree BLOB NOT NULL
);

-- The keypairs in our wallet
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_keys (
	key_id INTEGER PRIMARY KEY NOT NULL,
	is_default INTEGER NOT NULL,
	public BLOB NOT NULL,
	secret BLOB NOT NULL
);

-- The coins we have the information to and can spend
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_coins (
	coin BLOB PRIMARY KEY NOT NULL,
	is_spent INTEGER NOT NULL,
	value BLOB NOT NULL,
	token_id BLOB NOT NULL,
	spend_hook BLOB NOT NULL,
	user_data BLOB NOT NULL,
	coin_blind BLOB NOT NULL,
	value_blind BLOB NOT NULL,
	token_blind BLOB NOT NULL,
	secret BLOB NOT NULL,
	nullifier BLOB NOT NULL,
	leaf_position BLOB NOT NULL,
	memo BLOB
);

-- Arbitrary tokens
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_tokens (
	mint_authority BLOB PRIMARY KEY NOT NULL,
	token_id BLOB NOT NULL,
	is_frozen INTEGER NOT NULL
);

-- The token aliases in our wallet
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_aliases (
	alias BLOB PRIMARY KEY NOT NULL,
	token_id BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_transactions_history (
	id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
	transaction_hash TEXT UNIQUE NOT NULL,
	status TEXT NOT NULL,
	tx TEXT UNIQUE NOT NULL
);
