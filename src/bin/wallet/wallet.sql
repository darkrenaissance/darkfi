ATTACH DATABASE 'wallet.db' AS wallet KEY 'testkey';
SELECT sqlcipher_export('wallet');
CREATE TABLE IF NOT EXISTS keys(
    key_id INT PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL,
    key_private BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS key_public on keys(key_public);

