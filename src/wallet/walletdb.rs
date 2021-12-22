use incrementalmerkletree::bridgetree::BridgeTree;
use std::{fs::create_dir_all, path::Path, str::FromStr};

use async_std::sync::Arc;
use log::{error, info, trace};
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
        merkle_node::MerkleNode,
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
        if password.trim().is_empty() {
            error!("Password is empty. You must set a password to use the wallet.");
            return Err(Error::from(ClientFailed::EmptyPassword))
        }

        if path != "sqlite::memory:" {
            let p = Path::new(path.strip_prefix("sqlite://").unwrap());
            if let Some(dirname) = p.parent() {
                info!("Creating path to database: {}", dirname.display());
                create_dir_all(&dirname)?;
            }
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
        info!("Initializing wallet database");
        let tree = include_str!("../../sql/tree.sql");
        let keys = include_str!("../../sql/keys.sql");
        let coins = include_str!("../../sql/coins.sql");

        let mut conn = self.conn.acquire().await?;

        trace!("Initalizing merkle tree table");
        sqlx::query(tree).execute(&mut conn).await?;

        trace!("Initializing keys table");
        sqlx::query(keys).execute(&mut conn).await?;

        trace!("Initializing coins table");
        sqlx::query(coins).execute(&mut conn).await?;
        Ok(())
    }

    pub async fn key_gen(&self) -> Result<()> {
        trace!("Attempting to generate keypairs");
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
        trace!("Writing keypair into the wallet database");
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
        trace!("Returning keypairs");
        let mut conn = self.conn.acquire().await?;

        // TODO: Think about multiple keys
        let row = sqlx::query("SELECT * FROM keys").fetch_one(&mut conn).await?;
        let public: PublicKey = self.get_value_deserialized(row.get("public"))?;
        let secret: SecretKey = self.get_value_deserialized(row.get("secret"))?;

        Ok(vec![Keypair { public, secret }])
    }

    pub async fn tree_gen(&self) -> Result<()> {
        trace!("Attempting to generate merkle tree");
        let mut conn = self.conn.acquire().await?;

        match sqlx::query("SELECT * FROM tree").fetch_one(&mut conn).await {
            Ok(_) => {
                error!("Tree already exist");
                Err(Error::from(ClientFailed::TreeExists))
            }
            Err(_) => {
                let tree = BridgeTree::<MerkleNode, 32>::new(100);
                self.put_tree(tree).await?;
                Ok(())
            }
        }
    }

    pub async fn get_tree(&self) -> Result<BridgeTree<MerkleNode, 32>> {
        trace!("Getting merkle tree");
        let mut conn = self.conn.acquire().await?;

        let row = sqlx::query("SELECT tree FROM tree").fetch_one(&mut conn).await?;
        let tree: BridgeTree<MerkleNode, 32> = bincode::deserialize(row.get("tree"))?;
        Ok(tree)
    }

    pub async fn put_tree(&self, tree: BridgeTree<MerkleNode, 32>) -> Result<()> {
        trace!("Attempting to write merkle tree");
        let mut conn = self.conn.acquire().await?;

        let tree_bytes = bincode::serialize(&tree)?;
        sqlx::query("INSERT INTO tree(tree) VALUES (?1)")
            .bind(tree_bytes)
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn get_own_coins(&self) -> Result<OwnCoins> {
        trace!("Finding own coins");
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

            let secret = self.get_value_deserialized(row.get("secret"))?;
            let nullifier = self.get_value_deserialized(row.get("nullifier"))?;

            let oc = OwnCoin { coin, note, secret, nullifier };

            own_coins.push(oc);
        }

        Ok(own_coins)
    }

    pub async fn put_own_coins(&self, own_coin: OwnCoin) -> Result<()> {
        trace!("Putting own coin into wallet database");
        let coin = self.get_value_serialized(&own_coin.coin.to_bytes())?;
        let serial = self.get_value_serialized(&own_coin.note.serial)?;
        let coin_blind = self.get_value_serialized(&own_coin.note.coin_blind)?;
        let value_blind = self.get_value_serialized(&own_coin.note.value_blind)?;
        let value = own_coin.note.value.to_le_bytes();
        let token_id = self.get_value_serialized(&own_coin.note.token_id)?;
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
        trace!("Removing own coins from wallet database");
        let mut conn = self.conn.acquire().await?;
        sqlx::query("DROP TABLE coins;").execute(&mut conn).await?;
        Ok(())
    }

    pub async fn confirm_spend_coin(&self, coin: &Coin) -> Result<()> {
        trace!("Confirm spend coin");
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
        trace!("Getting tokens and balances");
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
            trace!("Did not find any unspent coins");
        }

        Ok(Balances { list })
    }

    pub async fn get_token_id(&self) -> Result<Vec<DrkTokenId>> {
        trace!("Getting token ID");
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
        trace!("Checking if token ID exists");

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
        trace!("Testing wallet");
        let mut conn = self.conn.acquire().await?;
        let _row = sqlx::query("SELECT * FROM keys").fetch_one(&mut conn).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::merkle_node::MerkleNode,
        types::{DrkCoinBlind, DrkSerial, DrkValueBlind},
    };
    use incrementalmerkletree::bridgetree::BridgeTree;
    use pasta_curves::{arithmetic::Field, pallas};
    use rand::rngs::OsRng;

    const WPASS: &str = "darkfi";

    fn dummy_coin(s: &SecretKey, v: u64, t: &DrkTokenId) -> OwnCoin {
        let serial = DrkSerial::random(&mut OsRng);
        let note = Note {
            serial,
            value: v,
            token_id: *t,
            coin_blind: DrkCoinBlind::random(&mut OsRng),
            value_blind: DrkValueBlind::random(&mut OsRng),
        };

        let coin = Coin(pallas::Base::random(&mut OsRng));
        let nullifier = Nullifier::new(*s, serial);

        OwnCoin { coin, note, secret: *s, nullifier }
    }

    #[async_std::test]
    async fn test_walletdb() -> Result<()> {
        let wallet = WalletDb::new("sqlite::memory:", WPASS.to_string()).await?;
        let keypair = Keypair::random(&mut OsRng);
        let tree1 = BridgeTree::<MerkleNode, 32>::new(100);

        // init_db()
        wallet.init_db().await?;

        // put_keypair()
        wallet.put_keypair(&keypair.public, &keypair.secret).await?;

        let token_id = DrkTokenId::random(&mut OsRng);

        let c0 = dummy_coin(&keypair.secret, 69, &token_id);
        let c1 = dummy_coin(&keypair.secret, 420, &token_id);
        let c2 = dummy_coin(&keypair.secret, 42, &token_id);
        let c3 = dummy_coin(&keypair.secret, 11, &token_id);

        // put_own_coins()
        wallet.put_own_coins(c0).await?;
        wallet.put_own_coins(c1).await?;
        wallet.put_own_coins(c2).await?;
        wallet.put_own_coins(c3).await?;

        // put_tree()
        wallet.put_tree(tree1).await?;

        // get_token_id()
        let id = wallet.get_token_id().await?;
        assert_eq!(id.len(), 4);

        for i in id {
            assert_eq!(i, token_id);
            assert!(wallet.token_id_exists(i).await?);
        }

        // get_balances()
        let balances = wallet.get_balances().await?;
        assert_eq!(balances.list.len(), 4);
        assert_eq!(balances.list[1].value, 420);
        assert_eq!(balances.list[2].value, 42);
        assert_eq!(balances.list[3].token_id, token_id);

        // get_keypairs()
        let keypair_r = wallet.get_keypairs().await?[0];
        assert_eq!(keypair, keypair_r);

        // get_own_coins()
        let own_coins = wallet.get_own_coins().await?;
        assert_eq!(own_coins.len(), 4);
        assert_eq!(own_coins[0], c0);
        assert_eq!(own_coins[1], c1);
        assert_eq!(own_coins[2], c2);
        assert_eq!(own_coins[3], c3);

        // get_tree()
        let tree2 = wallet.get_tree().await?;
        assert_eq!(tree1, tree2);

        Ok(())
    }
}
