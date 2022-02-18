use std::{fs::create_dir_all, path::Path, str::FromStr, time::Duration};

use async_std::sync::Arc;
use incrementalmerkletree::bridgetree::BridgeTree;
use log::{debug, error, info, LevelFilter};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    ConnectOptions, Row, SqlitePool,
};

use super::wallet_api::WalletApi;

use crate::{
    crypto::{
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        types::DrkTokenId,
    },
    node::client::ClientFailed,
    util::NetworkName,
    Error, Result,
};

pub type CashierDbPtr = Arc<CashierDb>;

#[derive(Debug, Clone)]
pub struct TokenKey {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

pub struct WithdrawToken {
    pub token_public_key: Vec<u8>,
    pub network: NetworkName,
    pub token_id: DrkTokenId,
    pub mint_address: String,
}

pub struct DepositToken {
    pub drk_public_key: PublicKey,
    pub token_key: TokenKey,
    pub token_id: DrkTokenId,
    pub mint_address: String,
}

pub struct CashierDb {
    pub conn: SqlitePool,
}

impl WalletApi for CashierDb {}

impl CashierDb {
    pub async fn new(path: &str, password: &str) -> Result<CashierDbPtr> {
        debug!("new() Constructor called");
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

        let mut connect_opts = SqliteConnectOptions::from_str(path)?
            .pragma("key", password.to_string())
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Off);

        connect_opts.log_statements(LevelFilter::Trace);
        connect_opts.log_slow_statements(LevelFilter::Trace, Duration::from_micros(10));

        let conn = SqlitePoolOptions::new().connect_with(connect_opts).await?;

        info!("Opened connection at path: {:?}", path);
        Ok(Arc::new(CashierDb { conn }))
    }

    pub async fn init_db(&self) -> Result<()> {
        let main_kps = include_str!("../../../sql/cashier_main_keypairs.sql");
        let deposit_kps = include_str!("../../../sql/cashier_deposit_keypairs.sql");
        let withdraw_kps = include_str!("../../../sql/cashier_withdraw_keypairs.sql");

        let mut conn = self.conn.acquire().await?;

        debug!("Initializing main keypairs table");
        sqlx::query(main_kps).execute(&mut conn).await?;

        debug!("Initializing deposit keypairs table");
        sqlx::query(deposit_kps).execute(&mut conn).await?;

        debug!("Initializing withdraw keypairs table");
        sqlx::query(withdraw_kps).execute(&mut conn).await?;
        Ok(())
    }

    pub async fn tree_gen(&self) -> Result<()> {
        debug!("Attempting to generate merkle tree");
        let mut conn = self.conn.acquire().await?;

        match sqlx::query("SELECT * FROM tree").fetch_one(&mut conn).await {
            Ok(_) => {
                error!("Tree already exists");
                Err(Error::from(ClientFailed::TreeExists))
            }
            Err(_) => {
                let tree = BridgeTree::<MerkleNode, 32>::new(100);
                self.put_tree(&tree).await?;
                Ok(())
            }
        }
    }

    pub async fn get_tree(&self) -> Result<BridgeTree<MerkleNode, 32>> {
        debug!("Getting merkle tree");
        let mut conn = self.conn.acquire().await?;

        let row = sqlx::query("SELECT tree FROM tree").fetch_one(&mut conn).await?;
        let tree: BridgeTree<MerkleNode, 32> = bincode::deserialize(row.get("tree"))?;
        Ok(tree)
    }

    pub async fn put_tree(&self, tree: &BridgeTree<MerkleNode, 32>) -> Result<()> {
        debug!("Attempting to write merkle tree");
        let mut conn = self.conn.acquire().await?;

        let tree_bytes = bincode::serialize(tree)?;
        sqlx::query("INSERT INTO tree(tree) VALUES (?1)")
            .bind(tree_bytes)
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn put_main_keys(&self, token_key: &TokenKey, network: &NetworkName) -> Result<()> {
        debug!("Writing main keys into the database");
        let network = self.get_value_serialized(network)?;

        let mut conn = self.conn.acquire().await?;
        sqlx::query(
            "INSERT INTO main_keypairs
            (token_key_secret, token_key_public, network)
            VALUES
            (?1, ?2, ?3);",
        )
        .bind(token_key.secret_key.clone())
        .bind(token_key.public_key.clone())
        .bind(network)
        .execute(&mut conn)
        .await?;

        Ok(())
    }

    pub async fn get_main_keys(&self, network: &NetworkName) -> Result<Vec<TokenKey>> {
        debug!("Returning main keypairs");
        let network = self.get_value_serialized(network)?;

        let mut conn = self.conn.acquire().await?;

        let rows = sqlx::query(
            "SELECT token_key_secret, token_key_public
             FROM main_keypairs WHERE network = ?1;",
        )
        .bind(network)
        .fetch_all(&mut conn)
        .await?;

        let mut keys = vec![];
        for row in rows {
            let secret_key = row.get("token_key_secret");
            let public_key = row.get("token_key_public");
            keys.push(TokenKey { secret_key, public_key })
        }

        Ok(keys)
    }

    pub async fn remove_withdraw_and_deposit_keys(&self) -> Result<()> {
        debug!("Removing withdraw and deposit keys");
        let mut conn = self.conn.acquire().await?;
        sqlx::query("DROP TABLE deposit_keypairs;").execute(&mut conn).await?;
        sqlx::query("DROP TABLE withdraw_keypairs;").execute(&mut conn).await?;

        Ok(())
    }

    pub async fn put_withdraw_keys(
        &self,
        token_key_public: &[u8],
        d_key_public: &PublicKey,
        d_key_secret: &SecretKey,
        network: &NetworkName,
        token_id: &DrkTokenId,
        mint_address: String,
    ) -> Result<()> {
        debug!("Writing withdraw keys to database");
        let public = self.get_value_serialized(d_key_public)?;
        let secret = self.get_value_serialized(d_key_secret)?;
        let network = self.get_value_serialized(network)?;
        let token_id = self.get_value_serialized(token_id)?;
        let confirm = self.get_value_serialized(&false)?;
        let mint_address = self.get_value_serialized(&mint_address)?;

        let mut conn = self.conn.acquire().await?;
        sqlx::query(
            "INSERT INTO withdraw_keypairs
            (token_key_public, d_key_secret, d_key_public,
             network, token_id, mint_address, confirm)
            VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7);",
        )
        .bind(token_key_public)
        .bind(secret)
        .bind(public)
        .bind(network)
        .bind(token_id)
        .bind(mint_address)
        .bind(confirm)
        .execute(&mut conn)
        .await?;

        Ok(())
    }

    pub async fn put_deposit_keys(
        &self,
        d_key_public: &PublicKey,
        token_key_secret: &[u8],
        token_key_public: &[u8],
        network: &NetworkName,
        token_id: &DrkTokenId,
        mint_address: String,
    ) -> Result<()> {
        debug!("Writing deposit keys to database");
        let d_key_public = self.get_value_serialized(d_key_public)?;
        let token_id = self.get_value_serialized(token_id)?;
        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&false)?;
        let mint_address = self.get_value_serialized(&mint_address)?;

        let mut conn = self.conn.acquire().await?;
        sqlx::query(
            "INSERT INTO deposit_keypairs
            (d_key_public, token_key_secret, token_key_public,
             network, token_id, mint_address, confirm)
            VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7);",
        )
        .bind(d_key_public)
        .bind(token_key_secret)
        .bind(token_key_public)
        .bind(network)
        .bind(token_id)
        .bind(mint_address)
        .bind(confirm)
        .execute(&mut conn)
        .await?;

        Ok(())
    }

    pub async fn get_withdraw_private_keys(&self) -> Result<Vec<SecretKey>> {
        debug!("Getting withdraw private keys");
        let confirm = self.get_value_serialized(&false)?;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query(
            "SELECT d_key_secret FROM withdraw_keypairs
             WHERE confirm = ?1",
        )
        .bind(confirm)
        .fetch_all(&mut conn)
        .await?;

        let mut secret_keys = vec![];
        for row in rows {
            let key: SecretKey = self.get_value_deserialized(row.get("d_key_secret"))?;
            secret_keys.push(key);
        }

        Ok(secret_keys)
    }

    pub async fn get_withdraw_token_public_key_by_dkey_public(
        &self,
        pubkey: &PublicKey,
    ) -> Result<Option<WithdrawToken>> {
        debug!("Get token address by pubkey");
        let d_key_public = self.get_value_serialized(pubkey)?;
        let confirm = self.get_value_serialized(&false)?;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query(
            "SELECT token_key_public, network, token_id, mint_address
             FROM withdraw_keypairs
             WHERE d_key_public = ?1
             AND confirm = ?2;",
        )
        .bind(d_key_public)
        .bind(confirm)
        .fetch_all(&mut conn)
        .await?;

        let mut token_addrs = vec![];
        for row in rows {
            let token_public_key = row.get("token_key_public");
            let network = self.get_value_deserialized(row.get("network"))?;
            let token_id = self.get_value_deserialized(row.get("token_id"))?;
            let mint_address = self.get_value_deserialized(row.get("mint_address"))?;

            token_addrs.push(WithdrawToken { token_public_key, network, token_id, mint_address });
        }

        Ok(token_addrs.pop())
    }

    pub async fn get_deposit_token_keys_by_dkey_public(
        &self,
        d_key_public: &PublicKey,
        network: &NetworkName,
    ) -> Result<Vec<TokenKey>> {
        debug!("Checking for existing dkey");
        let d_key_public = self.get_value_serialized(d_key_public)?;
        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&false)?;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query(
            "SELECT token_key_secret, token_key_public
             FROM deposit_keypairs
             WHERE d_key_public = ?1
             AND network = ?2
             AND confirm = ?3;",
        )
        .bind(d_key_public)
        .bind(network)
        .bind(confirm)
        .fetch_all(&mut conn)
        .await?;

        let mut keys = vec![];
        for row in rows {
            let secret_key = row.get("token_key_secret");
            let public_key = row.get("token_key_public");
            keys.push(TokenKey { secret_key, public_key });
        }

        Ok(keys)
    }

    pub async fn get_withdraw_keys_by_token_public_key(
        &self,
        token_key_public: &[u8],
        network: &NetworkName,
    ) -> Result<Option<Keypair>> {
        debug!("Checking for existing token address");
        let confirm = self.get_value_serialized(&false)?;
        let network = self.get_value_serialized(network)?;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query(
            "SELECT d_key_secret, d_key_public FROM withdraw_keypairs
             WHERE token_key_public = ?1
             AND network = ?2
             AND confirm = ?3;",
        )
        .bind(token_key_public)
        .bind(network)
        .bind(confirm)
        .fetch_all(&mut conn)
        .await?;

        let mut keypairs = vec![];
        for row in rows {
            let public = self.get_value_deserialized(row.get("d_key_public"))?;
            let secret = self.get_value_deserialized(row.get("d_key_secret"))?;
            keypairs.push(Keypair { public, secret });
        }

        Ok(keypairs.pop())
    }

    pub async fn confirm_withdraw_key_record(
        &self,
        token_address: &[u8],
        network: &NetworkName,
    ) -> Result<()> {
        debug!("Confirm withdraw keys");
        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&true)?;

        let mut conn = self.conn.acquire().await?;
        sqlx::query(
            "UPDATE withdraw_keypairs
             SET confirm = ?1
             WHERE token_key_public = ?2
             AND network = ?3;",
        )
        .bind(confirm)
        .bind(token_address)
        .bind(network)
        .execute(&mut conn)
        .await?;

        Ok(())
    }

    pub async fn confirm_deposit_key_record(
        &self,
        d_key_public: &PublicKey,
        network: &NetworkName,
    ) -> Result<()> {
        debug!("Confirm deposit keys");
        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&true)?;
        let d_key_public = self.get_value_serialized(d_key_public)?;

        let mut conn = self.conn.acquire().await?;
        sqlx::query(
            "UPDATE deposit_keypairs
             SET confirm = ?1
             WHERE d_key_public = ?2
             AND network = ?3;",
        )
        .bind(confirm)
        .bind(d_key_public)
        .bind(network)
        .execute(&mut conn)
        .await?;

        Ok(())
    }

    pub async fn get_deposit_token_keys_by_network(
        &self,
        network: &NetworkName,
    ) -> Result<Vec<DepositToken>> {
        debug!("Checking for existing dkey");
        let network = self.get_value_serialized(network)?;
        let confirm = self.get_value_serialized(&false)?;

        let mut conn = self.conn.acquire().await?;
        let rows = sqlx::query(
            "SELECT d_key_public, token_key_secret, token_key_public, token_id, mint_address
             FROM deposit_keypairs
             WHERE network = ?1
             AND confirm = ?2;",
        )
        .bind(network)
        .bind(confirm)
        .fetch_all(&mut conn)
        .await?;

        let mut keys = vec![];

        for row in rows {
            let drk_public_key = self.get_value_deserialized(row.get("d_key_public"))?;
            let secret_key = row.get("token_key_secret");
            let public_key = row.get("token_key_public");
            let token_id = self.get_value_deserialized(row.get("token_id"))?;
            let mint_address = self.get_value_deserialized(row.get("mint_address"))?;
            keys.push(DepositToken {
                drk_public_key,
                token_key: TokenKey { secret_key, public_key },
                token_id,
                mint_address,
            });
        }

        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::serial::serialize;
    use group::ff::Field;
    use rand::rngs::OsRng;

    const WPASS: &str = "darkfi";

    #[async_std::test]
    async fn test_cashierdb() -> Result<()> {
        let wallet = CashierDb::new("sqlite::memory:", WPASS).await?;

        // init_db()
        wallet.init_db().await?;

        // BTC testnet address
        let token_addr_secret = serialize(&String::from("2222222222222222222222222222222222"));
        let token_addr_public = serialize(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"));

        let keypair = Keypair::random(&mut OsRng);
        let token_id = DrkTokenId::random(&mut OsRng);

        let network = NetworkName::Bitcoin;

        // put_main_keys()
        wallet
            .put_main_keys(
                &TokenKey {
                    secret_key: token_addr_secret.clone(),
                    public_key: token_addr_public.clone(),
                },
                &network,
            )
            .await?;

        // get_main_keys()
        let keys = wallet.get_main_keys(&network).await?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].secret_key, token_addr_secret);
        assert_eq!(keys[0].public_key, token_addr_public);

        // put_deposit_keys()
        wallet
            .put_deposit_keys(
                &keypair.public,
                &token_addr_secret,
                &token_addr_public,
                &network,
                &token_id,
                String::new(),
            )
            .await?;

        // get_deposit_token_keys_by_dkey_public()
        let keys = wallet.get_deposit_token_keys_by_dkey_public(&keypair.public, &network).await?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].secret_key, token_addr_secret);
        assert_eq!(keys[0].public_key, token_addr_public);

        // get_deposit_token_keys_by_network()
        let resumed_keys = wallet.get_deposit_token_keys_by_network(&network).await?;
        assert_eq!(resumed_keys[0].drk_public_key, keypair.public);
        assert_eq!(resumed_keys[0].token_key.secret_key, token_addr_secret);
        assert_eq!(resumed_keys[0].token_key.public_key, token_addr_public);
        assert_eq!(resumed_keys[0].token_id, token_id);

        // confirm_deposit_key_record()
        wallet.confirm_deposit_key_record(&keypair.public, &network).await?;
        let keys = wallet.get_deposit_token_keys_by_dkey_public(&keypair.public, &network).await?;
        assert_eq!(keys.len(), 0);

        // put_withdraw_keys()
        wallet
            .put_withdraw_keys(
                &token_addr_public,
                &keypair.public,
                &keypair.secret,
                &network,
                &token_id,
                String::new(),
            )
            .await?;

        // get_withdraw_keys_by_token_public_key()
        let addr =
            wallet.get_withdraw_keys_by_token_public_key(&token_addr_public, &network).await?;
        assert!(addr.is_some());

        // confirm_withdraw_key_record()
        wallet.confirm_withdraw_key_record(&token_addr_public, &network).await?;
        let addr =
            wallet.get_withdraw_keys_by_token_public_key(&token_addr_public, &network).await?;
        assert!(addr.is_none());

        Ok(())
    }
}
