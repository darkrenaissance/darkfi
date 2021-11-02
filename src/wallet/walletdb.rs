use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use log::{debug, error, info};
use pasta_curves::arithmetic::Field;
use rand::rngs::OsRng;
use rusqlite::{named_params, params, Connection};

use super::WalletApi;
use crate::crypto::{coin::Coin, note::Note, nullifier::Nullifier, OwnCoin, OwnCoins};
use crate::{client::ClientFailed, serial, types::*, Error, Result};

pub type WalletPtr = Arc<WalletDb>;

#[derive(Debug, Clone)]
pub struct Keypair {
    pub public: DrkPublicKey,
    pub private: DrkSecretKey,
}

#[derive(Debug, Clone)]
pub struct Balance {
    pub token_id: DrkTokenId,
    pub value: u64,
    pub nullifier: Nullifier,
}

#[derive(Debug, Clone)]
pub struct Balances {
    pub list: Vec<Balance>,
}
impl Balances {
    pub fn add(&mut self, balance: &Balance) {
        if let Some(mut saved_balance) = self
            .list
            .iter_mut()
            .find(|b| b.token_id == balance.token_id)
        {
            saved_balance.value += balance.value;
        } else {
            self.list.push(balance.clone());
        }
    }
}

pub struct WalletDb {
    pub conn: Connection,
}

impl WalletApi for WalletDb {}

impl WalletDb {
    pub fn new(path: &Path, password: String) -> Result<WalletPtr> {
        debug!(target: "WALLETDB", "new() Constructor called");
        if password.trim().is_empty() {
            error!(target: "WALLETDB", "Password is empty. You must set a password to use the wallet.");
            return Err(Error::from(ClientFailed::EmptyPassword));
        }

        let conn = Connection::open(path)?;
        conn.pragma_update(None, "key", &password)?;
        info!(target: "WALLETDB", "Opened connection at path: {:?}", path);

        Ok(Arc::new(Self { conn }))
    }

    pub fn init_db(&self) -> Result<()> {
        debug!(target: "WALLETDB", "Initialize...");
        let contents = include_str!("../../sql/schema.sql");
        Ok(self.conn.execute_batch(contents)?)
    }

    pub fn key_gen(&self) -> Result<()> {
        debug!(target: "WALLETDB", "Attempting to generate keys...");
        let mut stmt = self.conn.prepare("SELECT * FROM keys WHERE key_id > ?")?;

        let key_check = stmt.exists(params!["0"])?;

        if !key_check {
            let secret = DrkSecretKey::random(&mut OsRng);
            let public = derive_publickey(secret);
            self.put_keypair(&public, &secret)?;
            return Ok(());
        }

        error!(target: "WALLETDB", "Keys already exist.");
        Err(Error::from(ClientFailed::KeyExists))
    }

    pub fn put_keypair(&self, key_public: &DrkPublicKey, key_private: &DrkSecretKey) -> Result<()> {
        debug!(target: "WALLETDB", "put_keypair()");
        let key_public = serial::serialize(key_public);
        let key_private = serial::serialize(key_private);

        self.conn.execute(
            "INSERT INTO keys(key_public, key_private) VALUES (?1, ?2)",
            params![key_public, key_private],
        )?;

        Ok(())
    }

    pub fn get_keypairs(&self) -> Result<Vec<Keypair>> {
        debug!(target: "WALLETDB", "Returning keypairs...");
        let mut stmt = self.conn.prepare("SELECT * FROM keys")?;

        // this just gets the first key. maybe we should randomize this
        let key_iter = stmt.query_map([], |row| Ok((row.get(1)?, row.get(2)?)))?;
        let mut keypairs = Vec::new();

        for key in key_iter {
            let key = key?;
            let public = key.0;
            let private = key.1;
            let public: DrkPublicKey = self.get_value_deserialized(public)?;
            let private: DrkSecretKey = self.get_value_deserialized(private)?;
            keypairs.push(Keypair { public, private });
        }

        Ok(keypairs)
    }

    pub fn get_own_coins(&self) -> Result<OwnCoins> {
        debug!(target: "WALLETDB", "Get own coins");
        let is_spent = 0;

        let mut coins = self
            .conn
            .prepare("SELECT * FROM coins WHERE is_spent = :is_spent ;")?;

        let rows = coins.query_map(&[(":is_spent", &is_spent)], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                // TODO: row.get(6)?,
                row.get(7)?,
                row.get(9)?,
            ))
        })?;

        let mut own_coins = Vec::new();

        for row in rows {
            let row = row?;
            let coin = self.get_value_deserialized(row.0)?;

            // note
            let serial = self.get_value_deserialized(row.1)?;
            let coin_blind = self.get_value_deserialized(row.2)?;
            let value_blind = self.get_value_deserialized(row.3)?;
            let value: u64 = row.4;
            let token_id = self.get_value_deserialized(row.5)?;

            let note = Note {
                serial,
                value,
                token_id,
                coin_blind,
                value_blind,
            };

            // TODO:
            // let witness = self.get_value_deserialized(row.6)?;
            // let secret: DrkSecretKey = self.get_value_deserialized(row.7)?;
            // let nullifier: Nullifier = self.get_value_deserialized(row.8)?;
            let secret: DrkSecretKey = self.get_value_deserialized(row.6)?;
            let nullifier: Nullifier = self.get_value_deserialized(row.7)?;

            let oc = OwnCoin {
                coin,
                note,
                secret,
                // TODO: witness,
                nullifier,
            };

            own_coins.push(oc)
        }

        Ok(own_coins)
    }

    pub fn put_own_coins(&self, own_coin: OwnCoin) -> Result<()> {
        debug!(target: "WALLETDB", "Put own coins");
        let coin = self.get_value_serialized(&own_coin.coin.to_bytes())?;
        let serial = self.get_value_serialized(&own_coin.note.serial)?;
        let coin_blind = self.get_value_serialized(&own_coin.note.coin_blind)?;
        let value_blind = self.get_value_serialized(&own_coin.note.value_blind)?;
        let value: u64 = own_coin.note.value;
        let token_id = self.get_value_serialized(&own_coin.note.token_id)?;
        // TODO: let witness = self.get_value_serialized(&own_coin.witness)?;
        let secret = self.get_value_serialized(&own_coin.secret)?;
        let is_spent = 0;
        let nullifier = self.get_value_serialized(&own_coin.nullifier)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO coins
            (coin, serial, value, token_id, coin_blind,
            valcom_blind, witness, secret, is_spent, nullifier)
            VALUES
            (:coin, :serial, :value, :token_id, :coin_blind,
             :valcom_blind, :witness, :secret, :is_spent, :nullifier);",
            named_params! {
                ":coin": coin,
                ":serial": serial,
                ":value": value,
                ":token_id": token_id,
                ":coin_blind": coin_blind,
                ":valcom_blind": value_blind,
                // TODO: ":witness": witness,
                ":secret": secret,
                ":is_spent": is_spent,
                ":nullifier": nullifier,
            },
        )?;

        Ok(())
    }

    pub fn remove_own_coins(&self) -> Result<()> {
        debug!(target: "WALLETDB", "Remove own coins");
        let _rows = self.conn.execute("DROP TABLE coins;", [])?;
        Ok(())
    }

    pub fn confirm_spend_coin(&self, coin: &Coin) -> Result<()> {
        debug!(target: "WALLETDB", "Confirm spend coin");
        let is_spent = 1;
        let coin = self.get_value_serialized(coin)?;

        self.conn.execute(
            "UPDATE coins
            SET is_spent = ?1
            WHERE coin = ?2 ;",
            params![is_spent, coin],
        )?;

        Ok(())
    }

    /* TODO:
    pub fn get_witnesses(&self) -> Result<HashMap<Vec<u8>, IncrementalWitness<MerkleNode>>> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        let is_spent = 0;

        let mut witnesses =
            conn.prepare("SELECT coin, witness FROM coins WHERE is_spent = :is_spent;")?;

        let rows = witnesses.query_map(&[(":is_spent", &is_spent)], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let mut witnesses = HashMap::new();
        for i in rows {
            let i = i?;
            let coin: Vec<u8> = i.0;
            let witness: IncrementalWitness<MerkleNode> = self.get_value_deserialized(i.1)?;
            witnesses.insert(coin, witness);
        }

        Ok(witnesses)
    }

    pub fn update_witnesses(
        &self,
        witnesses: HashMap<Vec<u8>, IncrementalWitness<MerkleNode>>,
    ) -> Result<()> {
        debug!(target: "WALLETDB", "Updating witness");

        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "key", &self.password)?;

        for (coin, witness) in witnesses.iter() {
            let witness = self.get_value_serialized(witness)?;
            let is_spent = 0;

            conn.execute(
                "UPDATE coins SET witness = ?1  WHERE coin = ?2 AND is_spent = ?3",
                params![witness, coin, is_spent],
            )?;
        }

        Ok(())
    }
    */

    pub fn get_balances(&self) -> Result<Balances> {
        debug!(target: "WALLETDB", "Get token and balances...");
        let is_spent = 0;

        let mut stmt = self.conn.prepare(
            "SELECT value, token_id, nullifier FROM coins  WHERE is_spent = :is_spent ;",
        )?;

        let rows = stmt.query_map(&[(":is_spent", &is_spent)], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

        let mut balances = Balances { list: Vec::new() };

        for row in rows {
            let row = row?;
            let value: u64 = row.0;
            let token_id: DrkTokenId = self.get_value_deserialized(row.1)?;
            let nullifier: Nullifier = self.get_value_deserialized(row.2)?;
            balances.add(&Balance {
                token_id,
                value,
                nullifier,
            });
        }

        Ok(balances)
    }

    pub fn get_token_id(&self) -> Result<Vec<DrkTokenId>> {
        debug!(target: "WALLETDB", "Get token ID...");
        let is_spent = 0;

        let mut stmt = self
            .conn
            .prepare("SELECT token_id FROM coins WHERE is_spent = :is_spent ;")?;

        let rows = stmt.query_map(&[(":is_spent", &is_spent)], |row| row.get(0))?;

        let mut token_ids = Vec::new();
        for row in rows {
            let row = row?;
            let token_id = self.get_value_deserialized(row).unwrap();

            token_ids.push(token_id);
        }

        Ok(token_ids)
    }

    pub fn token_id_exists(&self, token_id: &DrkTokenId) -> Result<bool> {
        debug!(target: "WALLETDB", "Check tokenID exists");
        let is_spent = 0;
        let id = self.get_value_serialized(token_id)?;

        let mut stmt = self
            .conn
            .prepare("SELECT * FROM coins WHERE token_id = ? AND is_spent = ? ;")?;

        let id_check = stmt.exists(params![id, is_spent])?;

        Ok(id_check)
    }

    pub fn test_wallet(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT * FROM keys")?;
        let _rows = stmt.query([])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // TODO: Clean up, there's a lot of duplicated code here.
    use super::*;
    use crate::crypto::{
        coin::Coin,
        types::{derive_publickey, CoinBlind, NullifierSerial, ValueCommitBlind},
        OwnCoin,
    };
    use crate::util::join_config_path;
    use ff::PrimeField;

    pub fn init_db(path: &Path, password: String) -> Result<()> {
        if !password.trim().is_empty() {
            let contents = include_str!("../../sql/schema.sql");
            let conn = Connection::open(path)?;
            debug!(target: "WALLETDB", "OPENED CONNECTION AT PATH {:?}", path);
            conn.pragma_update(None, "key", &password)?;
            conn.execute_batch(contents)?;
        } else {
            debug!(
                target: "WALLETDB", "Password is empty. You must set a password to use the wallet."
            );
            return Err(Error::from(ClientFailed::EmptyPassword));
        }
        Ok(())
    }

    #[test]
    pub fn test_get_token_id() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret = DrkSecretKey::random(&mut OsRng);
        let public = secret.derive_publickey();

        wallet.put_keypair(&public, &secret)?;

        let token_id = DrkTokenId::random(&mut OsRng);

        let note = Note {
            serial: NullifierSerial::random(&mut OsRng),
            value: 110,
            token_id,
            coin_blind: CoinBlind::random(&mut OsRng),
            valcom_blind: ValueCommitBlind::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        let nullifier = Nullifier::new(coin.repr);

        let own_coin = OwnCoin {
            coin,
            note,
            secret,
            witness,
            nullifier,
        };

        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin)?;

        let id = wallet.get_token_id()?;

        assert_eq!(id.len(), 1);

        for i in id {
            assert_eq!(i, token_id);
            assert!(wallet.token_id_exists(&i)?);
        }

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_get_balances() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test2_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret = DrkSecretKey::random(&mut OsRng);
        let public = secret.derive_publickey();

        wallet.put_keypair(&public, &secret)?;

        let token_id = DrkTokenId::random(&mut OsRng);

        let note = Note {
            serial: NullifierSerial::random(&mut OsRng),
            value: 110,
            token_id,
            coin_blind: CoinBlind::random(&mut OsRng),
            valcom_blind: ValueCommitBlind::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        let nullifier = Nullifier::new(coin.repr);

        let own_coin = OwnCoin {
            coin,
            note,
            secret,
            witness,
            nullifier,
        };

        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin)?;

        let balances = wallet.get_balances()?;

        assert_eq!(balances.list.len(), 1);
        assert_eq!(balances.list[0].value, 110);
        assert_eq!(balances.list[0].token_id, token_id);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_save_and_load_keypair() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test3_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret = DrkSecretKey::random(&mut OsRng);
        let public = secret.derive_publickey();

        wallet.put_keypair(&public, &secret)?;

        let keypair = wallet.get_keypairs()?[0].clone();

        assert_eq!(public, keypair.public);
        assert_eq!(secret, keypair.private);

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_put_and_get_own_coins() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test4_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret = DrkSecretKey::random(&mut OsRng);
        let public = secret.derive_publickey();

        wallet.put_keypair(&public, &secret)?;

        let note = Note {
            serial: NullifierSerial::random(&mut OsRng),
            value: 110,
            token_id: DrkTokenId::random(&mut OsRng),
            coin_blind: CoinBlind::random(&mut OsRng),
            valcom_blind: ValueCommitBlind::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();
        tree.append(MerkleNode::from_coin(&coin))?;

        let witness = IncrementalWitness::from_tree(&tree);

        let coin_ser = crate::serial::serialize(&coin.repr);

        assert_eq!(coin, crate::serial::deserialize(&coin_ser)?);

        let nullifier = Nullifier::new(coin.repr);

        let own_coin = OwnCoin {
            coin,
            note: note.clone(),
            secret,
            witness: witness.clone(),
            nullifier: nullifier.clone(),
        };

        wallet.put_own_coins(own_coin)?;

        let own_coin = wallet.get_own_coins()?[0].clone();

        assert_eq!(&own_coin.note.valcom_blind, &note.valcom_blind);
        assert_eq!(&own_coin.note.coin_blind, &note.coin_blind);
        assert_eq!(own_coin.secret, secret);
        assert_eq!(own_coin.witness.root(), witness.root());
        assert_eq!(own_coin.witness.path(), witness.path());
        assert_eq!(own_coin.nullifier, nullifier);

        wallet.confirm_spend_coin(&own_coin.coin)?;

        let own_coins = wallet.get_own_coins()?;

        assert_eq!(own_coins.len(), 0);

        wallet.put_own_coins(own_coin)?;

        let own_coins = wallet.get_own_coins()?;

        assert_eq!(own_coins.len(), 1);

        wallet.remove_own_coins()?;

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }

    #[test]
    pub fn test_get_witnesses_and_update_them() -> Result<()> {
        let walletdb_path = join_config_path(&PathBuf::from("test5_wallet.db"))?;
        let password: String = "darkfi".into();
        let wallet = WalletDb::new(&walletdb_path, password.clone())?;
        init_db(&walletdb_path, password)?;

        let secret = DrkSecretKey::random(&mut OsRng);
        let public = secret.derive_publickey();

        wallet.put_keypair(&public, &secret)?;

        let mut tree = crate::crypto::merkle::CommitmentTree::empty();

        let note = Note {
            serial: NullifierSerial::random(&mut OsRng),
            value: 110,
            token_id: DrkTokenId::random(&mut OsRng),
            coin_blind: CoinBlind::random(&mut OsRng),
            valcom_blind: ValueCommitBlind::random(&mut OsRng),
        };

        let coin = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let node = MerkleNode::from_coin(&coin);
        tree.append(node)?;
        tree.append(node)?;
        tree.append(node)?;
        tree.append(node)?;

        let witness = IncrementalWitness::from_tree(&tree);

        // for testing
        let nullifier = Nullifier::new(coin.repr);

        let own_coin = OwnCoin {
            coin,
            note,
            secret,
            witness,
            nullifier,
        };

        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin.clone())?;
        wallet.put_own_coins(own_coin)?;

        let coin2 = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());

        let node2 = MerkleNode::from_coin(&coin2);
        tree.append(node2)?;

        let mut updated_witnesses = wallet.get_witnesses()?;

        updated_witnesses.iter_mut().for_each(|(_, witness)| {
            witness.append(node2).expect("Append to witness");
        });

        wallet.update_witnesses(updated_witnesses)?;

        for (_, witness) in wallet.get_witnesses()?.iter() {
            assert_eq!(tree.root(), witness.root());
        }

        std::fs::remove_file(walletdb_path)?;

        Ok(())
    }
}
