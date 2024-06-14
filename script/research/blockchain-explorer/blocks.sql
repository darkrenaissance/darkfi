-- Database block table definition.
-- We store data in a usable format.
CREATE TABLE IF NOT EXISTS blocks (
    -- Header hash identifier of the block
    header_hash TEXT PRIMARY KEY NOT NULL,
    -- Block version
    version INTEGER NOT NULL,
    -- Previous block hash
    previous TEXT NOT NULL,
    -- Block height
    height INTEGER NOT NULL,
    -- Block creation timestamp
    timestamp INTEGER NOT NULL,
    -- The block's nonce. This value changes arbitrarily with mining.
    nonce INTEGER NOT NULL,
    -- Merkle tree root of the transactions hashes contained in this block
    root TEXT NOT NULL,
    -- Block producer signature
    signature BLOB NOT NULL
);

