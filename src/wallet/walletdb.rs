use crate::crypto::{coin::Coin, merkle::IncrementalWitness, merkle_node::MerkleNode, note::Note};
use crate::serial;
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::{Error, Result};

use async_std::sync::{Arc, Mutex};
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, params, Connection};

use std::path::PathBuf;

pub type WalletPtr = Arc<WalletDb>;
pub type OwnCoins = Vec<(Coin, Note, jubjub::Fr, IncrementalWitness<MerkleNode>)>;

pub struct WalletDb {
    pub path: PathBuf,
    pub secrets: Vec<jubjub::Fr>,
    pub cashier_secrets: Vec<jubjub::Fr>,
    pub coins: Mutex<Vec<Coin>>,
    pub notes: Mutex<Vec<Note>>,
    pub witnesses: Mutex<Vec<IncrementalWitness<MerkleNode>>>,
    pub cashier_public: jubjub::SubgroupPoint,
    pub public: jubjub::SubgroupPoint,
    pub password: String,
}

impl WalletDb {
    pub fn new(path: &std::path::PathBuf, password: String) -> Result<Self> {
        debug!(target: "walletdb", "new() Constructor called");
        let cashier_secret = jubjub::Fr::random(&mut OsRng);
        let secret = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;
        let coins = Mutex::new(Vec::new());
        let notes = Mutex::new(Vec::new());
        let witnesses = Mutex::new(Vec::new());
        Ok(Self {
            path: path.to_owned(),
            cashier_secrets: vec![cashier_secret.clone()],
            secrets: vec![secret.clone()],
            cashier_public,
            public,
            coins,
            notes,
            witnesses,
            password,
            //conn,
        })
    }

    pub fn init_db(&self) -> Result<()> {
        if !self.password.trim().is_empty() {
            let contents = include_str!("../../res/schema.sql");
            let conn = Connection::open(&self.path)?;
            debug!(target: "walletdb", "OPENED CONNECTION AT PATH {:?}", self.path);
            conn.pragma_update(None, "key", &self.password)?;
            conn.execute_batch(&contents)?;
        } else {
            info!("Password is empty. You must set a password to use the wallet.");
            info!("Current password: {}", self.password);
            return Err(Error::EmptyPassword);
        }
        Ok(())
    }

    pub fn init_cashier_db(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        debug!(target: "cashierdb", "OPENED CONNECTION AT PATH {:?}", self.path);
        let contents = include_str!("../../res/schema.sql");
        conn.execute_batch(&contents)?;
        Ok(())
    }

    pub fn get_own_coins(&self) -> Result<OwnCoins> {
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        let mut coins = conn.prepare("SELECT * FROM coins")?;
        let rows = coins.query_map([], |row| {
            let coin = self.get_value_deserialized(row.get(1)?).unwrap();

            // note
            let serial = self.get_value_deserialized(row.get(2)?).unwrap();
            let coin_blind = self.get_value_deserialized(row.get(3)?).unwrap();
            let valcom_blind = self.get_value_deserialized(row.get(4)?).unwrap();
            let value: u64 = row.get(5)?;
            let asset_id: u64 = row.get(6)?;

            let note = Note {
                serial,
                value,
                asset_id,
                coin_blind,
                valcom_blind,
            };

            let witness = self.get_value_deserialized(row.get(7)?).unwrap();
            let key_id: u64 = row.get(8)?;

            // return key_private from key_id
            let mut get_private_key =
                conn.prepare("SELECT key_private FROM keys WHERE key_id = :key_id")?;

            let rows = get_private_key.query_map(&[(":key_id", &key_id)], |row| row.get(0))?;

            let mut secret = Vec::new();
            for id in rows {
                secret.push(id?)
            }

            let secret: jubjub::Fr = self
                .get_value_deserialized(
                    secret
                        .pop()
                        .expect("unable to load public_key from walletdb"),
                )
                .unwrap();

            Ok((coin, note, secret, witness))
        })?;

        let mut own_coins = Vec::new();
        for id in rows {
            own_coins.push(id?)
        }

        Ok(own_coins)
    }

    pub fn put_own_coins(
        &self,
        coin: Coin,
        note: Note,
        witness: IncrementalWitness<MerkleNode>,
        secret: jubjub::Fr,
    ) -> Result<()> {
        // prepare the values
        let coin = self.get_value_serialized(&coin.repr)?;
        let serial = self.get_value_serialized(&note.serial)?;
        let coin_blind = self.get_value_serialized(&note.coin_blind)?;
        let valcom_blind = self.get_value_serialized(&note.valcom_blind)?;
        let value: u64 = note.value;
        let asset_id: u64 = note.asset_id;
        let witness = self.get_value_serialized(&witness)?;
        let secret = self.get_value_serialized(&secret)?;
        // open connection
        let conn = Connection::open(&self.path)?;
        // unlock database
        conn.pragma_update(None, "key", &self.password)?;

        // return key_id from key_private
        let mut get_id =
            conn.prepare("SELECT key_id FROM keys WHERE key_private = :key_private")?;

        let rows = get_id.query_map::<u64, _, _>(&[(":key_private", &secret)], |row| row.get(0))?;

        let mut key_id = Vec::new();
        for id in rows {
            key_id.push(id?)
        }

        conn.execute(
            "INSERT INTO coins(coin, serial, value, asset_id, coin_blind, valcom_blind, witness, key_id)
            VALUES (:coin, :serial, :value, :asset_id, :coin_blind, :valcom_blind, :witness, :key_id)",
            named_params! {
                ":coin": coin,
                ":serial": serial,
                ":value": value,
                ":asset_id": asset_id,
                ":coin_blind": coin_blind,
                ":valcom_blind": valcom_blind,
                ":witness": witness,
                ":key_id": key_id.pop().expect("key_id not found!"),
            },
        )?;
        Ok(())
    }

    pub fn key_gen(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "key_gen", "Attempting to generate keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (pubkey, privkey)
    }

    pub fn cash_key_gen(&self) -> (Vec<u8>, Vec<u8>) {
        debug!(target: "cash key_gen", "Generating cashier keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (pubkey, privkey)
    }

    pub fn put_keypair(&self, key_public: Vec<u8>, key_private: Vec<u8>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        conn.execute(
            "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
            params![key_public, key_private],
        )?;
        Ok(())
    }

    pub fn put_cashier_pub(&self, key_public: Vec<u8>) -> Result<()> {
        debug!(target: "save_cash_key", "Save cashier keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        conn.execute(
            "INSERT INTO cashier(key_public) VALUES (?1)",
            params![key_public],
        )?;
        Ok(())
    }

    pub fn get_public(&self) -> Result<jubjub::SubgroupPoint> {
        debug!(target: "get", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT key_public FROM keys")?;
        // this just gets the first key. maybe we should randomize this
        let key_iter = stmt.query_map([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key?);
        }
        let public: jubjub::SubgroupPoint = self.get_value_deserialized(
            pub_keys
                .pop()
                .expect("unable to load public_key from walletdb"),
        )?;

        Ok(public)
    }

    pub fn get_cashier_public(&self) -> Result<jubjub::SubgroupPoint> {
        debug!(target: "get_cashier_public", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT key_public FROM cashier")?;
        let key_iter = stmt.query_map([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key?);
        }
        let public: jubjub::SubgroupPoint = self.get_value_deserialized(
            pub_keys
                .pop()
                .expect("unable to load cashier public_key from walletdb"),
        )?;
        Ok(public)
    }

    pub fn get_private(&self) -> Result<jubjub::Fr> {
        debug!(target: "get", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT key_private FROM keys")?;
        let key_iter = stmt.query_map([], |row| row.get(0))?;
        let mut keys = Vec::new();
        for key in key_iter {
            keys.push(key?);
        }
        let private: jubjub::Fr = self.get_value_deserialized(
            keys.pop()
                .expect("unable to load private key from walletdb"),
        )?;
        Ok(private)
    }

    pub fn test_wallet(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT * FROM keys")?;
        let _rows = stmt.query([])?;
        Ok(())
    }

    fn get_tables_name(&self) -> Result<Vec<String>> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
        let table_iter = stmt.query_map::<String, _, _>([], |row| row.get(0))?;

        let mut tables = Vec::new();

        for table in table_iter {
            tables.push(table?);
        }

        Ok(tables)
    }

    pub fn destory(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        for table in self.get_tables_name()?.iter() {
            let drop_stmt = format!("DROP TABLE IF EXISTS {}", table);
            let drop_stmt = drop_stmt.as_str();
            conn.execute(drop_stmt, [])?;
        }

        Ok(())
    }
    pub fn get_value_serialized<T: Encodable>(&self, data: &T) -> Result<Vec<u8>> {
        let v = serialize(data);
        Ok(v)
    }

    pub fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::util::join_config_path;
    use ff::PrimeField;

    #[test]
    pub fn test_save_and_load_keypair() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test_wallet.db"))?;
        let wallet = WalletDb::new(&walletdb_path, "darkfi".into())?;
        wallet.init_db()?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let public2 = wallet.get_public()?;
        let secret2 = wallet.get_private()?;

        assert_eq!(public, public2);
        assert_eq!(secret, secret2);

        wallet.destory()?;

        Ok(())
    }

    #[test]
    pub fn test_put_and_get_own_coins() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test_wallet.db"))?;
        let wallet = WalletDb::new(&walletdb_path, "darkfi".into())?;
        wallet.init_db()?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let note = Note {
            serial: jubjub::Fr::random(&mut OsRng),
            value: 110,
            asset_id: 1,
            coin_blind: jubjub::Fr::random(&mut OsRng),
            valcom_blind: jubjub::Fr::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        wallet.put_own_coins(coin.clone(), note.clone(), witness.clone(), secret)?;

        let own_coin = wallet.get_own_coins()?[0].clone();

        assert_eq!(&own_coin.1.valcom_blind, &note.valcom_blind);
        assert_eq!(&own_coin.1.coin_blind, &note.coin_blind);
        assert_eq!(own_coin.2, secret);
        assert_eq!(own_coin.3.root(), witness.root());
        assert_eq!(own_coin.3.path(), witness.path());

        wallet.destory()?;

        Ok(())
    }

    //#[test]
    //    let password = "roseiscool2021";
    //    let path = join_config_path(&PathBuf::from("wallet.db"))?;
    //    let contents = include_str!("../../res/schema.sql");
    //    let conn = Connection::open(&path)?;
    //    debug!(target: "walletdb", "OPENED CONNECTION AT PATH {:?}", path);
    //    conn.pragma_update(None, "key", &password)?;
    //    conn.execute_batch(&contents)?;
    //    Ok(())
    //}

    //#[test]
    //pub fn test_keypair() -> Result<()> {
    //    let path = join_config_path(&PathBuf::from("wallet.db"))?;
    //    let conn = Connection::open(path)?;
    //    let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    //    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
    //    let key_public = serial::serialize(&public);
    //    let key_private = serial::serialize(&secret);
    //    let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
    //    let _rows = stmt.query([])?;
    //    conn.execute(
    //        "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
    //        params![key_public, key_private],
    //    )?;
    //    Ok(())
    //}

    //#[test]
    //pub fn test_get_id() -> Result<()> {
    //    let path = join_config_path(&PathBuf::from("wallet.db"))?;
    //    let conn = Connection::open(path)?;
    //    let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    //    let key_private = serial::serialize(&secret);
    //    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
    //    let key_public = serial::serialize(&public);
    //    let mut stmt = conn.prepare("PRAGMA key = 'testkey'")?;
    //    let _rows = stmt.query([])?;
    //    conn.execute(
    //        "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
    //        params![key_public, key_private],
    //    )?;
    //    let mut get_id =
    //        conn.prepare("SELECT key_id FROM keys WHERE key_private = :key_private")?;
    //    let rows =
    //        get_id.query_map::<u8, _, _>(&[(":key_private", &key_private)], |row| row.get(0))?;
    //    let mut key_id = Vec::new();
    //    for id in rows {
    //        key_id.push(id?)
    //    }
    //    println!("FOUND ID: {:?}", key_id.pop().unwrap());
    //    Ok(())
    //}

    //#[test]
    //pub fn test_own_coins() -> Result<()> {
    //    let key_private = Vec::new();
    //    let coin = Vec::new();
    //    let serial = Vec::new();
    //    let coin_blind = Vec::new();
    //    let valcom_blind = Vec::new();
    //    let value = Vec::new();
    //    let asset_id = Vec::new();
    //    let witness = Vec::new();
    //    let path = join_config_path(&PathBuf::from("wallet.db"))?;
    //    let conn = Connection::open(path)?;
    //    let contents = include_str!("../../res/schema.sql");
    //    match conn.execute_batch(&contents) {
    //        Ok(v) => println!("Database initalized successfully {:?}", v),
    //        Err(err) => println!("Error: {}", err),
    //    };
    //    //let mut unlock = conn.prepare("PRAGMA key = 'testkey'")?;
    //    //let _rows = unlock.query([])?;
    //    let mut get_id =
    //        conn.prepare("SELECT key_id FROM keys WHERE key_private = :key_private")?;
    //    let rows =
    //        get_id.query_map::<u8, _, _>(&[(":key_private", &key_private)], |row| row.get(0))?;
    //    let mut key_id = Vec::new();
    //    for id in rows {
    //        key_id.push(id?)
    //    }
    //    conn.execute(
    //        "INSERT INTO coins(coin, serial, value, asset_id, coin_blind, valcom_blind, witness, key_id)
    //        VALUES (:coin, :serial, :value, :asset_id, :coin_blind, :valcom_blind, :witness, :key_id)",
    //        named_params! {
    //            ":coin": coin,
    //            ":serial": serial,
    //            ":value": value,
    //            ":asset_id": asset_id,
    //            ":coin_blind": coin_blind,
    //            ":valcom_blind": valcom_blind,
    //            ":witness": witness,
    //            ":key_id": key_id.pop().expect("key_id not found!"),
    //        },
    //    )?;
    //    Ok(())
    //}
}
