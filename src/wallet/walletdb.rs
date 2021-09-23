use super::WalletApi;
use crate::client::ClientFailed;
use crate::crypto::{
    merkle::IncrementalWitness, merkle_node::MerkleNode, note::Note, OwnCoin, OwnCoins,
};
use crate::serial;
use crate::{Error, Result};

use async_std::sync::Arc;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::{named_params, params, Connection};

use std::path::{Path, PathBuf};

pub type WalletPtr = Arc<WalletDb>;

#[derive(Debug, Clone)]
pub struct Keypair {
    pub public: jubjub::SubgroupPoint,
    pub private: jubjub::Fr,
}

#[derive(Clone)]
pub struct WalletDb {
    pub path: PathBuf,
    pub password: String,
}

impl WalletApi for WalletDb {
    fn get_password(&self) -> String {
        self.password.to_owned()
    }
    fn get_path(&self) -> PathBuf {
        self.path.to_owned()
    }
}

impl WalletDb {
    pub fn new(path: &Path, password: String) -> Result<WalletPtr> {
        debug!(target: "WALLETDB", "new() Constructor called");
        Ok(Arc::new(Self {
            path: path.to_owned(),
            password,
        }))
    }

    pub fn init_db(&self) -> Result<()> {
        if !self.password.trim().is_empty() {
            let contents = include_str!("../../sql/schema.sql");
            let conn = Connection::open(&self.path)?;
            debug!(target: "WALLETDB", "OPENED CONNECTION AT PATH {:?}", self.path);
            conn.pragma_update(None, "key", &self.password)?;
            conn.execute_batch(&contents)?;
        } else {
            debug!(target: "WALLETDB", "Password is empty. You must set a password to use the wallet.");
            return Err(Error::from(ClientFailed::EmptyPassword));
        }
        Ok(())
    }

    pub fn key_gen(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        debug!(target: "WALLETDB", "Attempting to generate keys...");
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        self.put_keypair(pubkey.clone(), privkey.clone())?;
        Ok((pubkey, privkey))
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
    pub fn get_keypairs(&self) -> Result<Vec<Keypair>> {
        debug!(target: "WALLETDB", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT * FROM keys")?;
        // this just gets the first key. maybe we should randomize this
        let key_iter = stmt.query_map([], |row| Ok((row.get(1)?, row.get(2)?)))?;
        let mut keypairs = Vec::new();
        for key in key_iter {
            let key = key?;
            let public = key.0;
            let private = key.1;
            let public: jubjub::SubgroupPoint =
                self.get_value_deserialized::<jubjub::SubgroupPoint>(public)?;
            let private: jubjub::Fr = self.get_value_deserialized::<jubjub::Fr>(private)?;
            keypairs.push(Keypair { public, private });
        }

        if keypairs.is_empty() {
            return Err(Error::from(ClientFailed::DoNotHaveKeypair));
        }

        Ok(keypairs)
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
            let asset_id = self.get_value_deserialized(row.get(6)?).unwrap();

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
                .get_value_deserialized(secret.pop().expect("Load public_key from walletdb"))
                .unwrap();

            Ok(OwnCoin {
                coin,
                note,
                secret,
                witness,
            })
        })?;

        let mut own_coins = Vec::new();
        for id in rows {
            own_coins.push(id?)
        }

        Ok(own_coins)
    }

    pub fn put_own_coins(&self, own_coin: OwnCoin) -> Result<()> {
        // prepare the values
        let coin = self.get_value_serialized(&own_coin.coin.repr)?;
        let serial = self.get_value_serialized(&own_coin.note.serial)?;
        let coin_blind = self.get_value_serialized(&own_coin.note.coin_blind)?;
        let valcom_blind = self.get_value_serialized(&own_coin.note.valcom_blind)?;
        let value: u64 = own_coin.note.value;
        let asset_id = self.get_value_serialized(&own_coin.note.asset_id)?;
        let witness = self.get_value_serialized(&own_coin.witness)?;
        let secret = self.get_value_serialized(&own_coin.secret)?;
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
                ":key_id": key_id.pop().expect("Get key_id"),
            },
        )?;
        Ok(())
    }

    pub fn get_witnesses(&self) -> Result<Vec<(u64, IncrementalWitness<MerkleNode>)>> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let mut witnesses = conn.prepare("SELECT coin_id, witness FROM coins;")?;

        let rows = witnesses.query_map([], |row| {
            let coin_id: u64 = row.get(0)?;
            let witness: IncrementalWitness<MerkleNode> =
                self.get_value_deserialized(row.get(1)?).unwrap();
            Ok((coin_id, witness))
        })?;

        let mut witnesses = Vec::new();
        for i in rows {
            witnesses.push(i?)
        }

        Ok(witnesses)
    }

    pub fn update_witness(
        &self,
        coin_id: u64,
        witness: IncrementalWitness<MerkleNode>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let witness = self.get_value_serialized(&witness)?;

        conn.execute(
            "UPDATE coins SET witness = ?1  WHERE coin_id = ?2;",
            params![witness, coin_id],
        )?;

        Ok(())
    }

    pub fn put_cashier_pub(&self, key_public: Vec<u8>) -> Result<()> {
        debug!(target: "WALLETDB", "Save cashier keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        conn.execute(
            "INSERT INTO cashier(key_public) VALUES (?1)",
            params![key_public],
        )?;
        Ok(())
    }

    pub fn get_cashier_public_keys(&self) -> Result<Vec<jubjub::SubgroupPoint>> {
        debug!(target: "WALLETDB", "Returning keys...");
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT key_public FROM cashier")?;
        let key_iter = stmt.query_map([], |row| row.get(0))?;
        let mut pub_keys = Vec::new();
        for key in key_iter {
            let public: jubjub::SubgroupPoint = self.get_value_deserialized(key?)?;
            pub_keys.push(public);
        }

        if pub_keys.is_empty() {
            return Err(Error::from(ClientFailed::DoNotHaveCashierPublicKey));
        }

        Ok(pub_keys)
    }

    pub fn test_wallet(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;
        let mut stmt = conn.prepare("SELECT * FROM keys")?;
        let _rows = stmt.query([])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::crypto::{coin::Coin, OwnCoin};
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

        let keypair = wallet.get_keypairs()?[0].clone();

        assert_eq!(public, keypair.public);
        assert_eq!(secret, keypair.private);

        wallet.destroy()?;

        Ok(())
    }

    #[test]
    pub fn test_put_and_get_own_coins() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test2_wallet.db"))?;
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
            asset_id: jubjub::Fr::random(&mut OsRng),
            coin_blind: jubjub::Fr::random(&mut OsRng),
            valcom_blind: jubjub::Fr::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        let own_coin = OwnCoin {
            coin,
            note: note.clone(),
            secret,
            witness: witness.clone(),
        };
        wallet.put_own_coins(own_coin.clone())?;

        let own_coin = wallet.get_own_coins()?[0].clone();

        assert_eq!(&own_coin.note.valcom_blind, &note.valcom_blind);
        assert_eq!(&own_coin.note.coin_blind, &note.coin_blind);
        assert_eq!(own_coin.secret, secret);
        assert_eq!(own_coin.witness.root(), witness.root());
        assert_eq!(own_coin.witness.path(), witness.path());

        wallet.destroy()?;

        Ok(())
    }

    #[test]
    pub fn test_get_witnesses_and_update_them() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test3_wallet.db"))?;
        let wallet = WalletDb::new(&walletdb_path, "darkfi".into())?;
        wallet.init_db()?;

        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let key_public = serial::serialize(&public);
        let key_private = serial::serialize(&secret);

        wallet.put_keypair(key_public, key_private)?;

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();

        let note = Note {
            serial: jubjub::Fr::random(&mut OsRng),
            value: 110,
            asset_id: jubjub::Fr::random(&mut OsRng),
            coin_blind: jubjub::Fr::random(&mut OsRng),
            valcom_blind: jubjub::Fr::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let node = MerkleNode::from_coin(&coin);
        tree.append(node)?;
        tree.append(node)?;
        tree.append(node)?;
        tree.append(node)?;

        let witness = IncrementalWitness::from_tree(&tree);

        let own_coin = OwnCoin {
            coin,
            note,
            secret,
            witness,
        };

        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;

        let coin2 = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let node2 = MerkleNode::from_coin(&coin2);
        tree.append(node2)?;

        for (coin_id, witness) in wallet.get_witnesses()?.iter_mut() {
            witness.append(node2).expect("Append to witness");
            wallet.update_witness(coin_id.clone(), witness.clone())?;
        }

        for (_, witness) in wallet.get_witnesses()?.iter() {
            assert_eq!(tree.root(), witness.root());
        }

        wallet.destroy()?;

        Ok(())
    }
}
