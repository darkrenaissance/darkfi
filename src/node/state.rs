use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use log::{debug, error};

use crate::{
    blockchain::{rocks::columns, RocksColumn},
    crypto::{
        coin::Coin,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        proof::VerifyingKey,
        OwnCoin,
    },
    error,
    tx::Transaction,
    wallet::walletdb::WalletPtr,
    Result,
};

pub trait ProgramState {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool;
    fn is_valid_merkle(&self, merkle: &MerkleNode) -> bool;
    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool;
    fn mint_vk(&self) -> &VerifyingKey;
    fn spend_vk(&self) -> &VerifyingKey;
}

pub struct StateUpdate {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
    pub enc_notes: Vec<EncryptedNote>,
}

pub type VerifyResult<T> = std::result::Result<T, VerifyFailed>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum VerifyFailed {
    #[error("Invalid cashier public key for clear input {0}")]
    InvalidCashierKey(usize),
    #[error("Invalid merkle root for input {0}")]
    InvalidMerkle(usize),
    #[error("Duplicate nullifier for input {0}")]
    DuplicateNullifier(usize),
    #[error("Spend proof for input {0}")]
    SpendProof(usize),
    #[error("Mint proof for input {0}")]
    MintProof(usize),
    #[error("Invalid signature for clear input {0}")]
    ClearInputSignature(usize),
    #[error("Invalid signature for input {0}")]
    InputSignature(usize),
    #[error("Money in does not match money out (value commits)")]
    MissingFunds,
    #[error("Assets don't match some inputs or outputs (token commits)")]
    AssetMismatch,
    #[error("Inetrnal error: {0}")]
    InternalError(String),
}

impl From<error::Error> for VerifyFailed {
    fn from(err: error::Error) -> Self {
        VerifyFailed::InternalError(err.to_string())
    }
}

pub fn state_transition<S: ProgramState>(state: &S, tx: Transaction) -> VerifyResult<StateUpdate> {
    // Check deposits are legit
    debug!(target: "STATE TRANSITION", "iterate clear_inputs");

    for (i, input) in tx.clear_inputs.iter().enumerate() {
        // Check the public key in the clear inputs
        // It should be a valid public key for the cashier
        if !state.is_valid_cashier_public_key(&input.signature_public) {
            error!(target: "STATE TRANSITION", "Invalid cashier public key");
            return Err(VerifyFailed::InvalidCashierKey(i))
        }
    }

    debug!(target: "STATE TRANSITION", "iterate inputs");

    for (i, input) in tx.inputs.iter().enumerate() {
        let merkle = &input.revealed.merkle_root;

        // Merkle is used to know whether this is a coin that existed
        // in a previous state.
        if !state.is_valid_merkle(merkle) {
            return Err(VerifyFailed::InvalidMerkle(i))
        }

        // The nullifiers should not already exist
        // It is double spend protection.
        let nullifier = &input.revealed.nullifier;

        if state.nullifier_exists(nullifier) {
            return Err(VerifyFailed::DuplicateNullifier(i))
        }
    }

    debug!(target: "STATE TRANSITION", "Check the tx verifies correctly");
    tx.verify(state.mint_vk(), state.spend_vk())?;
    debug!(target: "STATE TRANSITION", "Verified successfully");

    let mut nullifiers = vec![];
    for input in tx.inputs {
        nullifiers.push(input.revealed.nullifier);
    }

    // Newly created coins for this tx
    let mut coins = vec![];
    let mut enc_notes = vec![];
    for output in tx.outputs {
        // Gather all the coins
        coins.push(output.revealed.coin);
        enc_notes.push(output.enc_note);
    }

    Ok(StateUpdate { nullifiers, coins, enc_notes })
}

pub struct State {
    /// The entire Merkle tree state
    pub tree: BridgeTree<MerkleNode, 32>,
    /// List of all previous and the current merkle roots.
    /// This is the hashed value of all the children.
    pub merkle_roots: RocksColumn<columns::MerkleRoots>,
    /// Nullifiers prevent double-spending
    pub nullifiers: RocksColumn<columns::Nullifiers>,
    /// List of Cashier public keys
    pub public_keys: Vec<PublicKey>,
    /// Verifying key for the Mint contract
    pub mint_vk: VerifyingKey,
    /// Verifying key for the Spend contract
    pub spend_vk: VerifyingKey,
}

impl State {
    pub async fn apply(
        &mut self,
        update: StateUpdate,
        secret_keys: Vec<SecretKey>,
        notify: Option<async_channel::Sender<(PublicKey, u64)>>,
        wallet: WalletPtr,
    ) -> Result<()> {
        // Extend our list of nullifiers with the ones from the update.
        debug!("Extend nullifiers");
        for nullifier in update.nullifiers {
            self.nullifiers.put(nullifier, vec![] as Vec<u8>)?;
        }

        debug!("Update Merkle tree and witness");
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.iter()) {
            // Add the new coins to the Merkle tree
            let node = MerkleNode(coin.0);
            self.tree.append(&node);

            // Keep track of all Merkle roots that have existed
            self.merkle_roots.put(self.tree.root(), vec![] as Vec<u8>)?;

            for secret in secret_keys.iter() {
                if let Some(note) = State::try_decrypt_note(enc_note, *secret) {
                    self.tree.witness();
                    let nullifier = Nullifier::new(*secret, note.serial);

                    let own_coin = OwnCoin { coin, note, secret: *secret, nullifier };

                    wallet.put_own_coins(own_coin).await?;

                    let pubkey = PublicKey::from_secret(*secret);

                    debug!("Received a coin: amount {}", note.value);
                    debug!("Send a notification");
                    if let Some(ch) = notify.clone() {
                        ch.send((pubkey, note.value)).await?;
                    }
                }
            }
            // Save updated merkle tree into wallet.
            wallet.put_tree(&self.tree).await?;
        }

        debug!("apply() exiting successfully");
        Ok(())
    }

    fn try_decrypt_note(ciphertext: &EncryptedNote, secret: SecretKey) -> Option<Note> {
        match ciphertext.decrypt(&secret) {
            Ok(note) => Some(note),
            Err(_) => None,
        }
    }
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        debug!("Check if it is a valid cashier public key");
        self.public_keys.contains(public)
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        debug!("Check if it is valid merkle");
        if let Ok(mr) = self.merkle_roots.key_exist(*merkle_root) {
            return mr
        }
        false
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        debug!("Check if nullifier exists");
        if let Ok(nl) = self.nullifiers.key_exist(nullifier.to_bytes()) {
            return nl
        }
        false
    }

    fn mint_vk(&self) -> &VerifyingKey {
        &self.mint_vk
    }

    fn spend_vk(&self) -> &VerifyingKey {
        &self.spend_vk
    }
}
