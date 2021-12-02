use std::str::FromStr;

use async_std::sync::Arc;
use log::{debug, error, info};
use rand::rngs::OsRng;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    Row, SqlitePool,
};

use crate::{
    client::ClientFailed,
    crypto::{
        coin::Coin,
        keypair::{Keypair, PublicKey, SecretKey},
        note::Note,
        nullifier::Nullifier,
        OwnCoin, OwnCoins,
    },
    serial::serialize,
    types::DrkTokenId,
    wallet::wallet_api::WalletApi,
    Error, Result,
};

pub type WalletPtr = Arc<WalletDb>;

#[derive(Clone, Debug)]
pub struct Balance {
    pub token_id: DrkTokenId,
    pub value: u64,
    pub nullifier: Nullifier,
}

#[derive(Clone, Debug)]
pub struct Balances {
    pub list: Vec<Balance>,
}

pub struct WalletDb {
    pub conn: SqlitePool,
}

impl WalletApi for WalletDb {}

impl WalletDb {
    pub async fn new(path: &str, password: String) -> Result<WalletPtr> {
        debug!("new() Constructor called");
        if password.trim().is_empty() {
            error!("Password is empty. You must set a password to use the wallet.");
            return Err(Error::from(ClientFailed::EmptyPassword))
        }

        let connect_opts = SqliteConnectOptions::from_str(path)?
            .pragma("key", password)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Off);

        let conn = SqlitePool::connect_with(connect_opts).await?;

        info!("Opened connection at path {}", path);
        Ok(Arc::new(WalletDb { conn }))
    }

    pub async fn init_db(&self) -> Result<()> {
        debug!("Initializing wallet database");
        let keys = include_str!("../../sql/keys.sql");
        let coins = include_str!("../../sql/coins.sql");

        let mut conn = self.conn.acquire().await?;

        debug!("Initializing keys table");
        sqlx::query(keys).execute(&mut conn).await?;

        debug!("Initializing coins table");
        sqlx::query(coins).execute(&mut conn).await?;
        Ok(())
    }

    pub async fn key_gen(&self) -> Result<()> {
        debug!("Attempting to generate keypairs");
        let mut conn = self.conn.acquire().await?;

        // TODO: Think about multiple keys
        match sqlx::query("SELECT * FROM keys WHERE key_id > ?").fetch_one(&mut conn).await {
            Ok(_) => {
                error!("Keys already exist");
                Err(Error::from(ClientFailed::KeyExists))
            }
            Err(_) => {
                let keypair = Keypair::random(&mut OsRng);
                self.put_keypair(&keypair.public, &keypair.secret).await?;
                Ok(())
            }
        }
    }

    pub async fn put_keypair(&self, public: &PublicKey, secret: &SecretKey) -> Result<()> {
        debug!("Writing keypair into the wallet database");
        let pubkey = serialize(&public.0);
        let secret = serialize(&secret.0);

        let mut conn = self.conn.acquire().await?;
        sqlx::query("INSERT INTO keys(public, secret) VALUES (?1, ?2)")
            .bind(pubkey)
            .bind(secret)
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn get_keypairs(&self) -> Result<Vec<Keypair>> {
        debug!("Returning keypairs");
        let mut conn = self.conn.acquire().await?;

        // TODO: Think about multiple keys
        let row = sqlx::query("SELECT * FROM keys").fetch_one(&mut conn).await?;
        let public: PublicKey = self.get_value_deserialized(row.get("public"))?;
        let secret: SecretKey = self.get_value_deserialized(row.get("secret"))?;

        Ok(vec![Keypair { public, secret }])
    }

    pub async fn get_own_coins(&self) -> Result<OwnCoins> {
        debug!("Finding own coins");
        let is_spent = 0;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query("SELECT * FROM coins WHERE is_spent = ?1;")
            .bind(is_spent)
            .fetch_all(&mut conn)
            .await?;

        let mut own_coins = vec![];
        for row in rows {
            let coin = self.get_value_deserialized(row.get("coin"))?;

            // Note
            let serial = self.get_value_deserialized(row.get("serial"))?;
            let coin_blind = self.get_value_deserialized(row.get("coin_blind"))?;
            let value_blind = self.get_value_deserialized(row.get("valcom_blind"))?;
            // TODO: FIXME:
            let value_bytes: Vec<u8> = row.get("value");
            let value = u64::from_le_bytes(value_bytes.try_into().unwrap());
            let token_id = self.get_value_deserialized(row.get("token_id"))?;

            let note = Note { serial, value, token_id, coin_blind, value_blind };

            // TODO:
            // let witness = deserialized(row.6)
            let secret = self.get_value_deserialized(row.get("secret"))?;
            let nullifier = self.get_value_deserialized(row.get("nullifier"))?;

            let oc = OwnCoin {
                coin,
                note,
                secret,
                // witness,
                nullifier,
            };

            own_coins.push(oc);
        }

        Ok(own_coins)
    }

    pub async fn put_own_coins(&self, own_coin: OwnCoin) -> Result<()> {
        debug!("Putting own coin into wallet database");
        let coin = self.get_value_serialized(&own_coin.coin.to_bytes())?;
        let serial = self.get_value_serialized(&own_coin.note.serial)?;
        let coin_blind = self.get_value_serialized(&own_coin.note.coin_blind)?;
        let value_blind = self.get_value_serialized(&own_coin.note.value_blind)?;
        let value = own_coin.note.value.to_le_bytes();
        let token_id = self.get_value_serialized(&own_coin.note.token_id)?;
        // TODO: let witness
        let secret = self.get_value_serialized(&own_coin.secret)?;
        let is_spent = 0;
        let nullifier = self.get_value_serialized(&own_coin.nullifier)?;

        let mut conn = self.conn.acquire().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO coins
            (coin, serial, value, token_id, coin_blind,
             valcom_blind, secret, is_spent, nullifier)
            VALUES
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);",
        )
        .bind(coin)
        .bind(serial)
        .bind(value.to_vec())
        .bind(token_id)
        .bind(coin_blind)
        .bind(value_blind)
        .bind(secret)
        .bind(is_spent)
        .bind(nullifier)
        .execute(&mut conn)
        .await?;

        Ok(())
    }

    pub async fn remove_own_coins(&self) -> Result<()> {
        debug!("Removing own coins from wallet database");
        let mut conn = self.conn.acquire().await?;
        sqlx::query("DROP TABLE coins;").execute(&mut conn).await?;
        Ok(())
    }

    pub async fn confirm_spend_coin(&self, coin: &Coin) -> Result<()> {
        debug!("Confirm spend coin");
        let is_spent = 1;
        let coin = self.get_value_serialized(coin)?;

        let mut conn = self.conn.acquire().await?;
        sqlx::query("UPDATE coins SET is_spent = ?1 WHERE coin = ?2;")
            .bind(is_spent)
            .bind(coin)
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn get_balances(&self) -> Result<Balances> {
        debug!("Getting tokens and balances");
        let is_spent = 0;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query("SELECT value, token_id, nullifier FROM coins WHERE is_spent = ?1;")
            .bind(is_spent)
            .fetch_all(&mut conn)
            .await?;

        let mut list = vec![];
        for row in rows {
            // TODO: FIXME:
            let value_bytes: Vec<u8> = row.get("value");
            let value = u64::from_le_bytes(value_bytes.try_into().unwrap());
            let token_id = self.get_value_deserialized(row.get("token_id"))?;
            let nullifier = self.get_value_deserialized(row.get("nullifier"))?;
            list.push(Balance { token_id, value, nullifier });
        }

        if list.is_empty() {
            debug!("Did not find any unspent coins");
        }

        Ok(Balances { list })
    }

    pub async fn get_token_id(&self) -> Result<Vec<DrkTokenId>> {
        debug!("Getting token ID");
        let is_spent = 0;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query("SELECT token_id FROM coins WHERE is_spent = ?1;")
            .bind(is_spent)
            .fetch_all(&mut conn)
            .await?;

        let mut token_ids = vec![];
        for row in rows {
            let token_id = self.get_value_deserialized(row.get("token_id"))?;
            token_ids.push(token_id);
        }

        Ok(token_ids)
    }

    pub async fn token_id_exists(&self, token_id: DrkTokenId) -> Result<bool> {
        debug!("Checking if token ID exists");
        let is_spent = 0;
        let id = self.get_value_serialized(&token_id)?;

        let mut conn = self.conn.acquire().await?;

        let id_check = sqlx::query("SELECT * FROM coins WHERE token_id = ?1 AND is_spent = ?2;")
            .bind(id)
            .bind(is_spent)
            .fetch_optional(&mut conn)
            .await?;

        Ok(id_check.is_some())
    }

    pub async fn test_wallet(&self) -> Result<()> {
        debug!("Testing wallet");
        let mut conn = self.conn.acquire().await?;
        let _row = sqlx::query("SELECT * FROM keys").fetch_one(&mut conn).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // TODO: Clean up, there's a lot of duplicated code here.
    use super::*;
    use crate::{
        crypto::{
            coin::Coin,
            types::{derive_public_key, CoinBlind, NullifierSerial, ValueCommitBlind},
            OwnCoin,
        },
        util::join_config_path,
    };
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
            return Err(Error::from(ClientFailed::EmptyPassword))
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
        let public = secret.derive_public_key();

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

        let own_coin = OwnCoin { coin, note, secret, witness, nullifier };

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
        let public = secret.derive_public_key();

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

        let own_coin = OwnCoin { coin, note, secret, witness, nullifier };

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
        let public = secret.derive_public_key();

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
        let public = secret.derive_public_key();

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
        let public = secret.derive_public_key();

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

        let own_coin = OwnCoin { coin, note, secret, witness, nullifier };

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
