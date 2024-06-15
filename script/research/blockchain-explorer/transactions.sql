-- Database transactions table definition.
-- We store data in a usable format.
CREATE TABLE IF NOT EXISTS transactions (
    -- Transaction hash identifier
    transaction_hash TEXT PRIMARY KEY NOT NULL,
    -- Header hash identifier of the block this transaction was included in
    header_hash TEXT NOT NULL,
    -- TODO: Split the payload into a more easily readable fields
    -- Transaction payload
    payload BLOB NOT NULL,

    FOREIGN KEY(header_hash) REFERENCES blocks(header_hash) ON DELETE CASCADE ON UPDATE CASCADE
);

