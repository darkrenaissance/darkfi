-- Wallet definitions for drk.
-- We store data that is needed for wallet operations.

PRAGMA foreign_keys = ON;

-- Scanned blocks information
CREATE TABLE IF NOT EXISTS scanned_blocks (
	height INTEGER PRIMARY KEY NOT NULL,
	hash TEXT NOT NULL,
	rollback_query TEXT NOT NULL
);

-- Transactions history
CREATE TABLE IF NOT EXISTS transactions_history (
    transaction_hash TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
	tx BLOB NOT NULL
);
