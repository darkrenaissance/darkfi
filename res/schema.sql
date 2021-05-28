ATTACH DATABASE 'wallet.db' AS wallet KEY 'testkey';
SELECT sqlcipher_export('wallet');
CREATE TABLE IF NOT EXISTS keys(
    key_id INT PRIMARY KEY NOT NULL,
    key_public BLOB NOT NULL,
    key_private BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS key_public on keys(key_public);
CREATE TABLE IF NOT EXISTS coins(
    coin BLOB NOT NULL,
    witness BLOB NOT NULL,
    serial BLOB NOT NULL,
    value INT NOT NULL,
    coin_blind BLOB NOT NULL,
    valcom_blind BLOB NOT NULL
);
