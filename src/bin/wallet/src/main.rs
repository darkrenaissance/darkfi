// rocksdb is the blockchain database
// it is a key value store
// sqlite is the encrypted wallet

use rusqlite::{Connection, Result};
use rocksdb::DB;

fn main() -> Result<()> {
    wallet()?;
    blockchain()?;
    Ok(())
}

fn wallet() -> Result<()> {
    let connector = connect()?;
    encrypt(&connector)?;
    println!("Created encrypted database.");
    decrypt(&connector)?;
    println!("Decrypted database.");
    Ok(())
}

fn connect() -> Result<Connection> {
    println!("Attempting to establish a connection...");
    let path = "/home/x/src/dbtest/src/wallet.db";
    let connector = Connection::open(&path);
    println!("Path created at {}", path);
    println!("Connection established");
    connector
}

fn encrypt(conn: &Connection) -> Result<()> {
    println!("Attempting to create an encrypted database...");
    conn.execute_batch(
        "ATTACH DATABASE 'encrypted.db' AS encrypted KEY 'testkey';
                SELECT sqlcipher_export('encrypted');
                DETACH DATABASE encrypted;",
    )
}

fn decrypt(conn: &Connection) -> Result<()> {
    println!("Attempting to decrypt database...");
    conn.execute_batch(
        "ATTACH DATABASE 'plaintext.db' AS plaintext KEY 'testkey';
                SELECT sqlcipher_export('plaintext');
                DETACH DATABASE plaintext;",
    )
}

fn blockchain() -> Result<()> {
    let db = create_db();
    write_db(&db)?;
    test_db(&db);
    Ok(())
}

fn create_db() -> DB {
    println!("Creating a blockchain database...");
    let path = "/home/x/src/dbtest/blockchain.db";
    let db = DB::open_default(path).unwrap();
    db
}

fn write_db(db: &DB) -> Result<()> {
    println!("Writing to the blockchain...");
    db.put(b"test-value", b"test-key").unwrap();
    Ok(())
}

fn test_db(db: &DB) {
    println!("Testing if write was successful...");
    match db.get(b"test-value") {
        Ok(Some(value)) => println!("retrieved value {}", String::from_utf8(value).unwrap()),
        Ok(None) => println!("value not found"),
        Err(e) => println!("operational problem encountered: {}", e),
    }
}

//fn configure_blockchain() {
//    let mut opts = Options::default();
//    let mut block_opts = BlockBasedOptions::default();
//    block_opts.set_index_type(BlockBasedIndexType::HashSearch);
//}
