-- Wallet definitions for drk.
-- We store data that is needed for wallet operations.

-- Broadcasted transactions history
CREATE TABLE IF NOT EXISTS transactions_history (
    transaction_hash TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
	tx BLOB NOT NULL
);
