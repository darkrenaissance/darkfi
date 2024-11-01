-- Wallet definitions for drk.
-- We store data that is needed for wallet operations.

PRAGMA foreign_keys = ON;

-- Arbitrary info that is potentially useful
CREATE TABLE IF NOT EXISTS wallet_info (
	last_scanned_block_height INTEGER NOT NULL,
	last_scanned_block_hash TEXT NOT NULL
);

-- Transactions history
CREATE TABLE IF NOT EXISTS transactions_history (
    transaction_hash TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
	tx BLOB NOT NULL
);
