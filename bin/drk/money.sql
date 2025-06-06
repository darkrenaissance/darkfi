-- Wallet definitions for this contract.
-- We store data that is needed to be able to receive and send tokens.

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
	leaf_position BLOB NOT NULL,
	memo BLOB,
	spent_tx_hash TEXT DEFAULT '-'
);

-- Arbitrary tokens
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_tokens (
	token_id BLOB PRIMARY KEY NOT NULL,
	mint_authority BLOB NOT NULL,
	token_blind BLOB NOT NULL,
	is_frozen INTEGER NOT NULL
);

-- The token aliases in our wallet
CREATE TABLE IF NOT EXISTS BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o_money_aliases (
	alias BLOB PRIMARY KEY NOT NULL,
	token_id BLOB NOT NULL
);
