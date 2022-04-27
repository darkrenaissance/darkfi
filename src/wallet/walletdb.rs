use std::{fs::create_dir_all, path::Path, str::FromStr, time::Duration};

use async_std::sync::Arc;
use incrementalmerkletree::bridgetree::BridgeTree;
use log::{debug, error, info, LevelFilter};
use rand::rngs::OsRng;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    ConnectOptions, Row, SqlitePool,
};

use crate::{
    crypto::{
        address::Address,
        coin::Coin,
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        note::Note,
        nullifier::Nullifier,
        types::DrkTokenId,
        OwnCoin, OwnCoins,
    },
    util::{
        expand_path,
        serial::{deserialize, serialize},
    },
    Error::{WalletEmptyPassword, WalletTreeExists},
    Result,
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

/// Helper function to initialize `WalletPtr`
pub async fn init_wallet(wallet_path: &str, wallet_pass: &str) -> Result<WalletPtr> {
    let expanded = expand_path(wallet_path)?;
    let wallet_path = format!("sqlite://{}", expanded.to_str().unwrap());
    let wallet = WalletDb::new(&wallet_path, wallet_pass).await?;
    Ok(wallet)
}

impl WalletDb {
    pub async fn new(path: &str, password: &str) -> Result<WalletPtr> {
        if password.trim().is_empty() {
            error!("Password is empty. You must set a password to use the wallet.");
            return Err(WalletEmptyPassword)
        }

        if path != "sqlite::memory:" {
            let p = Path::new(path.strip_prefix("sqlite://").unwrap());
            if let Some(dirname) = p.parent() {
                info!("Creating path to database: {}", dirname.display());
                create_dir_all(&dirname)?;
            }
        }

        let mut connect_opts = SqliteConnectOptions::from_str(path)?
            .pragma("key", password.to_string())
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Off);

        connect_opts.log_statements(LevelFilter::Trace);
        connect_opts.log_slow_statements(LevelFilter::Trace, Duration::from_micros(10));

        let conn = SqlitePool::connect_with(connect_opts).await?;

        info!("Opened connection at path {}", path);
        Ok(Arc::new(WalletDb { conn }))
    }

    pub async fn init_db(&self) -> Result<()> {
        info!("Initializing wallet database");
        let tree = include_str!("../../script/sql/tree.sql");
        let keys = include_str!("../../script/sql/keys.sql");
        let coins = include_str!("../../script/sql/coins.sql");

        let mut conn = self.conn.acquire().await?;

        debug!("Initializing merkle tree table");
        sqlx::query(tree).execute(&mut conn).await?;

        debug!("Initializing keys table");
        sqlx::query(keys).execute(&mut conn).await?;

        debug!("Initializing coins table");
        sqlx::query(coins).execute(&mut conn).await?;
        Ok(())
    }

    pub async fn keygen(&self) -> Result<Keypair> {
        debug!("Attempting to generate keypairs");
        let keypair = Keypair::random(&mut OsRng);
        self.put_keypair(&keypair).await?;
        Ok(keypair)
    }

    pub async fn put_keypair(&self, keypair: &Keypair) -> Result<()> {
        debug!("Writing keypair into the wallet database");
        let pubkey = serialize(&keypair.public);
        let secret = serialize(&keypair.secret);
        let is_default = 0;

        let mut conn = self.conn.acquire().await?;

        sqlx::query("INSERT INTO keys(public, secret, is_default) VALUES (?1, ?2, ?3)")
            .bind(pubkey)
            .bind(secret)
            .bind(is_default)
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn set_default_keypair(&self, public: &PublicKey) -> Result<Keypair> {
        debug!("Set default keypair");
        let mut conn = self.conn.acquire().await?;

        let pubkey = serialize(public);

        // unset previous default keypair
        sqlx::query("UPDATE keys SET is_default = 0;").execute(&mut conn).await?;

        // set new default keypair
        sqlx::query("UPDATE keys SET is_default = 1 WHERE public = ?1;")
            .bind(pubkey)
            .execute(&mut conn)
            .await?;

        let keypair = self.get_default_keypair().await?;
        Ok(keypair)
    }

    pub async fn get_default_keypair(&self) -> Result<Keypair> {
        debug!("Returning default keypair");
        let mut conn = self.conn.acquire().await?;

        let is_default: u32 = 1;

        let row = sqlx::query("SELECT * FROM keys WHERE is_default = ?1;")
            .bind(is_default)
            .fetch_one(&mut conn)
            .await?;

        let public: PublicKey = deserialize(row.get("public"))?;
        let secret: SecretKey = deserialize(row.get("secret"))?;

        Ok(Keypair { secret, public })
    }

    pub async fn get_default_address(&self) -> Result<Address> {
        debug!("Returning default address");
        let keypair = self.get_default_keypair_or_create_one().await?;

        Ok(Address::from(keypair.public))
    }

    pub async fn get_default_keypair_or_create_one(&self) -> Result<Keypair> {
        debug!("Returning default keypair or create one");

        let default_keypair = self.get_default_keypair().await;

        let keypair = if default_keypair.is_err() {
            let keypairs = self.get_keypairs().await?;
            let kp = if keypairs.is_empty() { self.keygen().await? } else { keypairs[0] };
            self.set_default_keypair(&kp.public).await?;
            kp
        } else {
            default_keypair?
        };

        Ok(keypair)
    }

    pub async fn get_keypairs(&self) -> Result<Vec<Keypair>> {
        debug!("Returning keypairs");
        let mut conn = self.conn.acquire().await?;

        let mut keypairs = vec![];

        for row in sqlx::query("SELECT * FROM keys").fetch_all(&mut conn).await? {
            let public: PublicKey = deserialize(row.get("public"))?;
            let secret: SecretKey = deserialize(row.get("secret"))?;
            keypairs.push(Keypair { public, secret });
        }

        Ok(keypairs)
    }

    pub async fn tree_gen(&self) -> Result<BridgeTree<MerkleNode, 32>> {
        debug!("Attempting to generate merkle tree");
        let mut conn = self.conn.acquire().await?;

        match sqlx::query("SELECT * FROM tree").fetch_one(&mut conn).await {
            Ok(_) => {
                error!("Merkle tree already exists");
                Err(WalletTreeExists)
            }
            Err(_) => {
                let tree = BridgeTree::<MerkleNode, 32>::new(100);
                self.put_tree(&tree).await?;
                Ok(tree)
            }
        }
    }

    pub async fn get_tree(&self) -> Result<BridgeTree<MerkleNode, 32>> {
        debug!("Getting merkle tree");
        let mut conn = self.conn.acquire().await?;

        let row = sqlx::query("SELECT * FROM tree").fetch_one(&mut conn).await?;
        let tree: BridgeTree<MerkleNode, 32> = bincode::deserialize(row.get("tree"))?;
        Ok(tree)
    }

    pub async fn put_tree(&self, tree: &BridgeTree<MerkleNode, 32>) -> Result<()> {
        debug!("Attempting to write merkle tree");
        let mut conn = self.conn.acquire().await?;

        let tree_bytes = bincode::serialize(tree)?;

        debug!("Deleting old row");
        sqlx::query("DELETE FROM tree;").execute(&mut conn).await?;

        debug!("Inserting new tree");
        sqlx::query("INSERT INTO tree (tree) VALUES (?1);")
            .bind(tree_bytes)
            .execute(&mut conn)
            .await?;

        Ok(())
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
            let coin = deserialize(row.get("coin"))?;

            // Note
            let serial = deserialize(row.get("serial"))?;
            let coin_blind = deserialize(row.get("coin_blind"))?;
            let value_blind = deserialize(row.get("valcom_blind"))?;
            // TODO: FIXME:
            let value_bytes: Vec<u8> = row.get("value");
            let value = u64::from_le_bytes(value_bytes.try_into().unwrap());
            let token_id = deserialize(row.get("token_id"))?;
            let note = Note { serial, value, token_id, coin_blind, value_blind };

            let secret = deserialize(row.get("secret"))?;
            let nullifier = deserialize(row.get("nullifier"))?;
            let leaf_position = deserialize(row.get("leaf_position"))?;

            let oc = OwnCoin { coin, note, secret, nullifier, leaf_position };

            own_coins.push(oc);
        }

        Ok(own_coins)
    }

    pub async fn put_own_coin(&self, own_coin: OwnCoin) -> Result<()> {
        debug!("Putting own coin into wallet database");

        let coin = serialize(&own_coin.coin.to_bytes());
        let serial = serialize(&own_coin.note.serial);
        let coin_blind = serialize(&own_coin.note.coin_blind);
        let value_blind = serialize(&own_coin.note.value_blind);
        let value = own_coin.note.value.to_le_bytes();
        let token_id = serialize(&own_coin.note.token_id);
        let secret = serialize(&own_coin.secret);
        let is_spent: u32 = 0;
        let nullifier = serialize(&own_coin.nullifier);
        let leaf_position = serialize(&own_coin.leaf_position);

        let mut conn = self.conn.acquire().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO coins
            (coin, serial, value, token_id, coin_blind,
             valcom_blind, secret, is_spent, nullifier,
             leaf_position)
            VALUES
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);",
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
        .bind(leaf_position)
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
        let is_spent: u32 = 1;
        let coin = serialize(coin);

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

        debug!("Found {} rows", rows.len());

        let mut list = vec![];
        for row in rows {
            // TODO: FIXME:
            let value_bytes: Vec<u8> = row.get("value");
            let value = u64::from_le_bytes(value_bytes.try_into().unwrap());
            let token_id = deserialize(row.get("token_id"))?;
            let nullifier = deserialize(row.get("nullifier"))?;
            list.push(Balance { token_id, value, nullifier });
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
            let token_id = deserialize(row.get("token_id"))?;
            token_ids.push(token_id);
        }

        Ok(token_ids)
    }

    pub async fn token_id_exists(&self, token_id: DrkTokenId) -> Result<bool> {
        debug!("Checking if token ID exists");

        let is_spent: u32 = 0;
        let id = serialize(&token_id);

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
    use super::*;
    use crate::crypto::{
        merkle_node::MerkleNode,
        types::{DrkCoinBlind, DrkSerial, DrkValueBlind},
    };
    use group::ff::Field;
    use incrementalmerkletree::{Frontier, Tree};
    use pasta_curves::pallas;
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
        let leaf_position: incrementalmerkletree::Position = 0.into();

        OwnCoin { coin, note, secret: *s, nullifier, leaf_position }
    }

    #[async_std::test]
    async fn test_walletdb() -> Result<()> {
        let wallet = WalletDb::new("sqlite::memory:", WPASS).await?;
        let keypair = Keypair::random(&mut OsRng);

        // init_db()
        wallet.init_db().await?;

        // tree_gen()
        let mut tree1 = wallet.tree_gen().await?;

        // put_keypair()
        wallet.put_keypair(&keypair).await?;

        let token_id = DrkTokenId::random(&mut OsRng);

        let c0 = dummy_coin(&keypair.secret, 69, &token_id);
        let c1 = dummy_coin(&keypair.secret, 420, &token_id);
        let c2 = dummy_coin(&keypair.secret, 42, &token_id);
        let c3 = dummy_coin(&keypair.secret, 11, &token_id);

        // put_own_coin()
        wallet.put_own_coin(c0).await?;
        tree1.append(&MerkleNode::from_coin(&c0.coin));
        tree1.witness();

        wallet.put_own_coin(c1).await?;
        tree1.append(&MerkleNode::from_coin(&c1.coin));
        tree1.witness();

        wallet.put_own_coin(c2).await?;
        tree1.append(&MerkleNode::from_coin(&c2.coin));
        tree1.witness();

        wallet.put_own_coin(c3).await?;
        tree1.append(&MerkleNode::from_coin(&c3.coin));
        tree1.witness();

        // We'll check this merkle root corresponds to the one we'll retrieve.
        let root1 = tree1.root();

        // put_tree()
        wallet.put_tree(&tree1).await?;

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

        /////////////////
        //// keypair ////
        /////////////////
        let keypair2 = Keypair::random(&mut OsRng);
        // add new keypair
        wallet.put_keypair(&keypair2).await?;
        // get all keypairs
        let keypairs = wallet.get_keypairs().await?;
        assert_eq!(keypair, keypairs[0]);
        assert_eq!(keypair2, keypairs[1]);
        // set the keypair at index 1 as the default keypair
        wallet.set_default_keypair(&keypair2.public).await?;
        // get default keypair
        assert_eq!(keypair2, wallet.get_default_keypair_or_create_one().await?);

        // get_own_coins()
        let own_coins = wallet.get_own_coins().await?;
        assert_eq!(own_coins.len(), 4);
        assert_eq!(own_coins[0], c0);
        assert_eq!(own_coins[1], c1);
        assert_eq!(own_coins[2], c2);
        assert_eq!(own_coins[3], c3);

        // get_tree()
        let tree2 = wallet.get_tree().await?;
        let root2 = tree2.root();
        assert_eq!(root1, root2);

        // Let's try it once more to test sql replacing.
        wallet.put_tree(&tree2).await?;
        let tree3 = wallet.get_tree().await?;
        let root3 = tree3.root();
        assert_eq!(root2, root3);

        Ok(())
    }
}
