/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! This module implements the client-side of this contract's interaction.
//! What we basically do here is implement an API that creates the necessary
//! structures and is able to export them to create a DarkFi Transaction
//! object that can be broadcasted to the network when we want to make a
//! payment with some coins in our wallet.
//! Note that this API doesn't involve any wallet interaction, but only
//! takes the necessary objects provided by the caller. This is so we can
//! abstract away the wallet interface to client implementations.

use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
use darkfi::{
    consensus::LeadCoin,
    zk::{Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    ClientFailed, Error, Result,
};
use darkfi_sdk::crypto::{
    diffie_hellman::{kdf_sapling, sapling_ka_agree},
    merkle_prelude::*,
    pallas,
    pasta_prelude::*,
    pedersen_commitment_base, pedersen_commitment_u64, poseidon_hash, Keypair, MerkleNode,
    MerklePosition, MerkleTree, Nullifier, PublicKey, SecretKey, TokenId, ValueBlind, ValueCommit,
};
use darkfi_serial::{serialize, Decodable, Encodable, SerialDecodable, SerialEncodable};
use halo2_proofs::circuit::Value;
use log::{debug, error, info};
use rand::rngs::OsRng;

use crate::model::{
    ClearInput, Input, MoneyStakeParams, MoneyTransferParams, MoneyUnstakeParams, Output,
    StakedInput, StakedOutput,
};

/// Client API for token minting and freezing
pub mod token_mint;

// Wallet SQL table constant names. These have to represent the SQL schema.
// TODO: They should also ideally be prefixed with the contract ID to avoid
//       collisions.
pub const MONEY_INFO_TABLE: &str = "money_info";
pub const MONEY_INFO_COL_LAST_SCANNED_SLOT: &str = "last_scanned_slot";

pub const MONEY_TREE_TABLE: &str = "money_tree";
pub const MONEY_TREE_COL_TREE: &str = "tree";

pub const MONEY_KEYS_TABLE: &str = "money_keys";
pub const MONEY_KEYS_COL_KEY_ID: &str = "key_id";
pub const MONEY_KEYS_COL_IS_DEFAULT: &str = "is_default";
pub const MONEY_KEYS_COL_PUBLIC: &str = "public";
pub const MONEY_KEYS_COL_SECRET: &str = "secret";

pub const MONEY_COINS_TABLE: &str = "money_coins";
pub const MONEY_COINS_COL_COIN: &str = "coin";
pub const MONEY_COINS_COL_IS_SPENT: &str = "is_spent";
pub const MONEY_COINS_COL_SERIAL: &str = "serial";
pub const MONEY_COINS_COL_VALUE: &str = "value";
pub const MONEY_COINS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_COINS_COL_SPEND_HOOK: &str = "spend_hook";
pub const MONEY_COINS_COL_USER_DATA: &str = "user_data";
pub const MONEY_COINS_COL_COIN_BLIND: &str = "coin_blind";
pub const MONEY_COINS_COL_VALUE_BLIND: &str = "value_blind";
pub const MONEY_COINS_COL_TOKEN_BLIND: &str = "token_blind";
pub const MONEY_COINS_COL_SECRET: &str = "secret";
pub const MONEY_COINS_COL_NULLIFIER: &str = "nullifier";
pub const MONEY_COINS_COL_LEAF_POSITION: &str = "leaf_position";
pub const MONEY_COINS_COL_MEMO: &str = "memo";

pub const MONEY_TOKENS_TABLE: &str = "money_tokens";
pub const MONEY_TOKENS_COL_SECRET: &str = "secret";
pub const MONEY_TOKENS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_TOKENS_COL_FROZEN: &str = "frozen";

pub const MONEY_ALIASES_TABLE: &str = "money_aliases";
pub const MONEY_ALIASES_COL_ALIAS: &str = "alias";
pub const MONEY_ALIASES_COL_TOKEN_ID: &str = "token_id";

/// Byte length of the AEAD tag of the chacha20 cipher used for note encryption
pub const AEAD_TAG_SIZE: usize = 16;

/// The `Coin` is represented as a base field element.
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Coin(pub pallas::Base);

impl Coin {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Try to create a `Coin` type from the given 32 bytes.
    /// Returns an error if the bytes don't fit in the base field.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self> {
        match pallas::Base::from_repr(bytes).into() {
            Some(v) => Ok(Self(v)),
            None => Err(Error::CoinFromBytes),
        }
    }
}

impl From<pallas::Base> for Coin {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

/// The `OwnCoin` is a representation of `Coin` with its respective metadata.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct OwnCoin {
    /// The coin hash
    pub coin: Coin,
    /// The attached Note
    pub note: Note,
    /// Coin's secret key
    pub secret: SecretKey,
    /// Coin's nullifier,
    pub nullifier: Nullifier,
    /// Coin's leaf position in the Merkle tree of coins
    pub leaf_position: MerklePosition,
}

/// The `Note` holds the inner attributes of a `Coin`
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Note {
    /// Serial number of the coin, used for the nullifier
    pub serial: pallas::Base,
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: TokenId,
    /// Spend hook used for protocol owned liquidity.
    /// Specifies which contract owns this coin.
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled.
    pub user_data: pallas::Base,
    /// Blinding factor for the coin bulla
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value pedersen commitment
    pub value_blind: ValueBlind,
    /// Blinding factor for the token ID pedersen commitment
    pub token_blind: ValueBlind,
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}

impl Note {
    /// Encrypt the note to some given `PublicKey` using an AEAD cipher.
    pub fn encrypt(&self, public_key: &PublicKey) -> Result<EncryptedNote> {
        let ephem_keypair = Keypair::random(&mut OsRng);
        let shared_secret = sapling_ka_agree(&ephem_keypair.secret, public_key);
        let key = kdf_sapling(&shared_secret, &ephem_keypair.public);

        let mut input = vec![];
        self.encode(&mut input)?;
        let input_len = input.len();

        let mut ciphertext = vec![0_u8; input_len + AEAD_TAG_SIZE];
        ciphertext[..input_len].copy_from_slice(&input);

        ChaCha20Poly1305::new(key.as_ref().into())
            .encrypt_in_place([0u8; 12][..].into(), &[], &mut ciphertext)
            .unwrap();

        Ok(EncryptedNote { ciphertext, ephem_public: ephem_keypair.public })
    }
}

/// The `EncryptedNote` represents a structure holding the ciphertext (which is
/// an encryption of the `Note` object, and the ephemeral `PublicKey` created at
/// the time when the encryption was done
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct EncryptedNote {
    /// Ciphertext of the encrypted `Note`
    pub ciphertext: Vec<u8>,
    /// Ephemeral public key created at the time of encrypting the note
    pub ephem_public: PublicKey,
}

impl EncryptedNote {
    /// Attempt to decrypt an `EncryptedNote` given a secret key.
    pub fn decrypt(&self, secret: &SecretKey) -> Result<Note> {
        let shared_secret = sapling_ka_agree(secret, &self.ephem_public);
        let key = kdf_sapling(&shared_secret, &self.ephem_public);

        let ciphertext_len = self.ciphertext.len();
        let mut plaintext = vec![0_u8; ciphertext_len];
        plaintext.copy_from_slice(&self.ciphertext);

        match ChaCha20Poly1305::new(key.as_ref().into()).decrypt_in_place(
            [0u8; 12][..].into(),
            &[],
            &mut plaintext,
        ) {
            Ok(()) => Ok(Note::decode(&plaintext[..ciphertext_len - AEAD_TAG_SIZE])?),
            Err(e) => Err(Error::NoteDecryptionFailed(e.to_string())),
        }
    }
}

// TODO: we can put all these in an internal module like:
// money_transfer::builder::ClearInputInfo

struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub signature_secret: SecretKey,
}

struct TransactionBuilderInputInfo {
    pub leaf_position: MerklePosition,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: Note,
}

struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub public_key: PublicKey,
}

pub struct TransferBurnRevealed {
    pub value_commit: ValueCommit,
    pub token_commit: ValueCommit,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

impl TransferBurnRevealed {
    #[allow(clippy::too_many_arguments)]
    pub fn compute(
        value: u64,
        token_id: TokenId,
        value_blind: ValueBlind,
        token_blind: ValueBlind,
        serial: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        user_data_blind: pallas::Base,
        coin_blind: pallas::Base,
        secret_key: SecretKey,
        leaf_position: MerklePosition,
        merkle_path: Vec<MerkleNode>,
        signature_secret: SecretKey,
    ) -> Self {
        let nullifier = Nullifier::from(poseidon_hash([secret_key.inner(), serial]));

        let public_key = PublicKey::from_secret(secret_key);
        let (pub_x, pub_y) = public_key.xy();

        let coin = poseidon_hash([
            pub_x,
            pub_y,
            pallas::Base::from(value),
            token_id.inner(),
            serial,
            spend_hook,
            user_data,
            coin_blind,
        ]);

        let merkle_root = {
            let position: u64 = leaf_position.into();
            let mut current = MerkleNode::from(coin);
            for (level, sibling) in merkle_path.iter().enumerate() {
                let level = level as u8;
                current = if position & (1 << level) == 0 {
                    MerkleNode::combine(level.into(), &current, sibling)
                } else {
                    MerkleNode::combine(level.into(), sibling, &current)
                };
            }
            current
        };

        let user_data_enc = poseidon_hash([user_data, user_data_blind]);

        let value_commit = pedersen_commitment_u64(value, value_blind);
        let token_commit = pedersen_commitment_base(token_id.inner(), token_blind);

        Self {
            value_commit,
            token_commit,
            nullifier,
            merkle_root,
            spend_hook,
            user_data_enc,
            signature_public: PublicKey::from_secret(signature_secret),
        }
    }

    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let tokcom_coords = self.token_commit.to_affine().coordinates().unwrap();
        let sigpub_coords = self.signature_public.inner().to_affine().coordinates().unwrap();

        // NOTE: It's important to keep this order the same as the `constrain_instance`
        //       calls in the zkas code.
        vec![
            self.nullifier.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            *tokcom_coords.x(),
            *tokcom_coords.y(),
            self.merkle_root.inner(),
            self.user_data_enc,
            *sigpub_coords.x(),
            *sigpub_coords.y(),
            // TODO: Why is spend_hook in the struct but not here?
        ]
    }
}

pub struct TransferMintRevealed {
    pub coin: Coin,
    pub value_commit: ValueCommit,
    pub token_commit: ValueCommit,
}

impl TransferMintRevealed {
    #[allow(clippy::too_many_arguments)]
    pub fn compute(
        value: u64,
        token_id: TokenId,
        value_blind: ValueBlind,
        token_blind: ValueBlind,
        serial: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        coin_blind: pallas::Base,
        public_key: PublicKey,
    ) -> Self {
        let value_commit = pedersen_commitment_u64(value, value_blind);
        let token_commit = pedersen_commitment_base(token_id.inner(), token_blind);

        let (pub_x, pub_y) = public_key.xy();

        let coin = Coin::from(poseidon_hash([
            pub_x,
            pub_y,
            pallas::Base::from(value),
            token_id.inner(),
            serial,
            spend_hook,
            user_data,
            coin_blind,
        ]));

        Self { coin, value_commit, token_commit }
    }

    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let tokcom_coords = self.token_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep this order the same as the `constrain_instance`
        //       calls in the zkas code.
        vec![
            self.coin.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            *tokcom_coords.x(),
            *tokcom_coords.y(),
        ]
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_transfer_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    value: u64,
    token_id: TokenId,
    value_blind: ValueBlind,
    token_blind: ValueBlind,
    serial: pallas::Base,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    coin_blind: pallas::Base,
    public_key: PublicKey,
) -> Result<(Proof, TransferMintRevealed)> {
    let revealed = TransferMintRevealed::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        spend_hook,
        user_data,
        coin_blind,
        public_key,
    );

    let (pub_x, pub_y) = public_key.xy();

    // NOTE: It's important to keep these in the same order as the zkas code.
    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Base(Value::known(token_id.inner())),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(coin_blind)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Scalar(Value::known(token_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &revealed.to_vec(), &mut OsRng)?;

    Ok((proof, revealed))
}

#[allow(clippy::too_many_arguments)]
pub fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    value: u64,
    token_id: TokenId,
    value_blind: ValueBlind,
    token_blind: ValueBlind,
    serial: pallas::Base,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    user_data_blind: pallas::Base,
    coin_blind: pallas::Base,
    secret_key: SecretKey,
    leaf_position: MerklePosition,
    merkle_path: Vec<MerkleNode>,
    signature_secret: SecretKey,
) -> Result<(Proof, TransferBurnRevealed)> {
    let revealed = TransferBurnRevealed::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        spend_hook,
        user_data,
        user_data_blind,
        coin_blind,
        secret_key,
        leaf_position,
        merkle_path.clone(),
        signature_secret,
    );

    // NOTE: It's important to keep these in the same order as the zkas code.
    let prover_witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Base(Value::known(token_id.inner())),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Scalar(Value::known(token_blind)),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(user_data_blind)),
        Witness::Base(Value::known(coin_blind)),
        Witness::Base(Value::known(secret_key.inner())),
        Witness::Uint32(Value::known(u64::from(leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.try_into().unwrap())),
        Witness::Base(Value::known(signature_secret.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &revealed.to_vec(), &mut OsRng)?;

    Ok((proof, revealed))
}

struct StakeLeadMintRevealed {
    pub value_commit: ValueCommit,
    pub pk: pallas::Base,
    pub commitment_x: pallas::Base,
    pub commitment_y: pallas::Base,
}

impl StakeLeadMintRevealed {
    pub fn compute(
        value: pallas::Base,
        pk: pallas::Base,
        value_blind: pallas::Scalar,
        commitment: pallas::Point,
    ) -> Self {
        let value_commit = pedersen_commitment_base(value, value_blind);
        let coord = commitment.to_affine().coordinates().unwrap();
        Self { value_commit, pk, commitment_x: *coord.x(), commitment_y: *coord.y() }
    }
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let value_coord = self.value_commit.to_affine().coordinates().unwrap();
        let value_cm_x = *value_coord.x();
        let value_cm_y = *value_coord.y();
        vec![value_cm_x, value_cm_y, self.pk, self.commitment_x, self.commitment_y]
    }
}

fn create_stake_mint_proof(
    zkbin: &ZkBinary, // LeadMint contract binary
    pk: &ProvingKey,
    public_key: pallas::Base,
    coin_commitment: pallas::Point,
    value: pallas::Base,
    value_blind: ValueBlind,
    coin_blind: ValueBlind,
    sk: pallas::Base,
    sk_root: pallas::Base,
    tau: pallas::Base,
    nonce: pallas::Base, // rho
) -> Result<(Proof, StakeLeadMintRevealed)> {
    let revealed = StakeLeadMintRevealed::compute(value, public_key, value_blind, coin_commitment);

    let prover_witnesses = vec![
        Witness::Base(Value::known(sk)),
        Witness::Base(Value::known(sk_root)),
        Witness::Base(Value::known(tau)),
        Witness::Base(Value::known(nonce)),
        Witness::Scalar(Value::known(coin_blind)),
        Witness::Base(Value::known(value)),
        Witness::Scalar(Value::known(value_blind)),
    ];
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &revealed.to_vec(), &mut OsRng)?;

    Ok((proof, revealed))
}

struct UnstakeLeadBurnRevealed {
    pub value_commit: ValueCommit,
    pub pk: pallas::Base,
    pub commitment_x: pallas::Base,
    pub commitment_y: pallas::Base,
    pub commitment_root: pallas::Base,
    pub sk_root: pallas::Base,
    pub nullifier: pallas::Base,
}

impl UnstakeLeadBurnRevealed {
    pub fn compute(
        value: pallas::Base,
        value_blind: ValueBlind,
        pk: pallas::Base,
        commitment: pallas::Point,
        commitment_root: pallas::Base,
        sk_root: pallas::Base,
        nullifier: pallas::Base,
    ) -> Self {
        let value_commit = pedersen_commitment_base(value, value_blind);
        let coord = commitment.to_affine().coordinates().unwrap();
        let commitment_x = *coord.x();
        let commitment_y = *coord.y();
        Self { value_commit, pk, commitment_x, commitment_y, commitment_root, sk_root, nullifier }
    }

    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let coord = self.value_commit.to_affine().coordinates().unwrap();
        let value_cm_x = *coord.x();
        let value_cm_y = *coord.y();
        vec![
            value_cm_x,
            value_cm_y,
            self.pk,
            self.commitment_x,
            self.commitment_y,
            self.commitment_root,
            self.sk_root,
            self.nullifier,
        ]
    }
}

fn create_unstake_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    value: pallas::Base,
    value_blind: ValueBlind,
    coin_blind: ValueBlind,
    public_key: pallas::Base,
    sk: pallas::Base,
    sk_root: pallas::Base,
    sk_pos: MerklePosition,
    sk_path: Vec<MerkleNode>,
    commitment_merkle_path: Vec<MerkleNode>,
    commitment: pallas::Point,
    commitment_root: pallas::Base,
    commitment_pos: MerklePosition,
    slot: u64,
    nonce: pallas::Base,
    nullifier: pallas::Base,
) -> Result<(Proof, UnstakeLeadBurnRevealed)> {
    let revealed = UnstakeLeadBurnRevealed::compute(
        value,
        value_blind,
        public_key,
        commitment,
        commitment_root,
        sk_root,
        nullifier,
    );

    let prover_witnesses = vec![
        Witness::MerklePath(Value::known(commitment_merkle_path.try_into().unwrap())),
        Witness::Uint32(Value::known(u64::from(commitment_pos).try_into().unwrap())), // u32
        Witness::Uint32(Value::known(u64::from(sk_pos).try_into().unwrap())),         // u32
        Witness::Base(Value::known(sk)),
        Witness::Base(Value::known(sk_root)),
        Witness::MerklePath(Value::known(sk_path.try_into().unwrap())),
        Witness::Base(Value::known(pallas::Base::from(slot))),
        Witness::Base(Value::known(nonce)),
        Witness::Scalar(Value::known(coin_blind)),
        Witness::Base(Value::known(value)),
        Witness::Scalar(Value::known(value_blind)),
    ];
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &revealed.to_vec(), &mut OsRng)?;

    Ok((proof, revealed))
}

/// Build half of the money contract OTC swap transaction parameters with the given data:
/// * `value_send` - Amount to send
/// * `token_id_send` - Token ID to send
/// * `value_recv` - Amount to receive
/// * `token_id_recv` - Token ID to receive
/// * `value_blinds` - Value blinds to use if we're the second half
/// * `token_blinds` - Token blinds to use if we're the second half
/// * `coins` - Set of coins we're able to spend
/// * `tree` - Current Merkle tree of coins
/// * `mint_zkbin` - ZkBinary of the mint circuit
/// * `mint_pk` - Proving key for the ZK mint proof
/// * `burn_zkbin` - ZkBinary of the burn circuit
/// * `burn_pk` - Proving key for the ZK burn proof
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn build_half_swap_tx(
    pubkey: &PublicKey,
    value_send: u64,
    token_id_send: TokenId,
    value_recv: u64,
    token_id_recv: TokenId,
    value_blinds: &[ValueBlind],
    token_blinds: &[ValueBlind],
    coins: &[OwnCoin],
    tree: &MerkleTree,
    mint_zkbin: &ZkBinary,
    mint_pk: &ProvingKey,
    burn_zkbin: &ZkBinary,
    burn_pk: &ProvingKey,
) -> Result<(
    MoneyTransferParams,
    Vec<Proof>,
    Vec<SecretKey>,
    Vec<OwnCoin>,
    Vec<ValueBlind>,
    Vec<ValueBlind>,
)> {
    debug!(target: "money", "Building OTC swap transaction half");
    assert!(value_send != 0);
    assert!(value_recv != 0);
    assert!(!coins.is_empty());

    debug!(target: "money", "Money::build_half_swap_tx(): Building anonymous inputs");
    // We'll take any coin that has correct value
    let Some(coin) = coins.iter().find(|x| x.note.value == value_send && x.note.token_id == token_id_send) else {
        error!(target: "money", "Money::build_half_swap_tx(): Did not find a coin with enough value to swap");
        return Err(ClientFailed::NotEnoughValue(value_send).into())
    };

    let leaf_position = coin.leaf_position;
    let root = tree.root(0).unwrap();
    let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();

    let input = TransactionBuilderInputInfo {
        leaf_position,
        merkle_path,
        secret: coin.secret,
        note: coin.note.clone(),
    };

    let spent_coins = vec![coin.clone()];

    let output = TransactionBuilderOutputInfo {
        value: value_recv,
        token_id: token_id_recv,
        public_key: *pubkey,
    };

    // We now fill this with necessary stuff
    let mut params = MoneyTransferParams { clear_inputs: vec![], inputs: vec![], outputs: vec![] };

    let val_blinds: Vec<ValueBlind>;
    let tok_blinds: Vec<ValueBlind>;

    // If we got non-empty `value_blinds` passed into this function, we use them here.
    // They should be sent to the second party by the swap initiator.
    let (value_send_blind, value_recv_blind) = {
        if value_blinds.is_empty() {
            let value_send_blind = ValueBlind::random(&mut OsRng);
            let value_recv_blind = ValueBlind::random(&mut OsRng);
            val_blinds = vec![value_send_blind, value_recv_blind];
            (value_send_blind, value_recv_blind)
        } else {
            val_blinds = vec![value_blinds[1], value_blinds[0]];
            (value_blinds[1], value_blinds[0])
        }
    };

    // The same goes for token blinds
    let (token_send_blind, token_recv_blind) = {
        if token_blinds.is_empty() {
            let token_send_blind = ValueBlind::random(&mut OsRng);
            let token_recv_blind = ValueBlind::random(&mut OsRng);
            tok_blinds = vec![token_send_blind, token_recv_blind];
            (token_send_blind, token_recv_blind)
        } else {
            tok_blinds = vec![token_blinds[1], token_blinds[0]];
            (token_blinds[1], token_blinds[0])
        }
    };

    // The ephemeral secret key we're using here.
    let signature_secret = SecretKey::random(&mut OsRng);

    // Disable composability for this old obsolete API
    let spend_hook = pallas::Base::zero();
    let user_data = pallas::Base::zero();
    let user_data_blind = pallas::Base::random(&mut OsRng);

    let mut zk_proofs = vec![];

    info!(target: "money", "Creating swap burn proof for input 0");
    let (proof, revealed) = create_transfer_burn_proof(
        burn_zkbin,
        burn_pk,
        input.note.value,
        input.note.token_id,
        value_send_blind,
        token_send_blind,
        input.note.serial,
        spend_hook,
        user_data,
        user_data_blind,
        input.note.coin_blind,
        input.secret,
        input.leaf_position,
        input.merkle_path,
        signature_secret,
    )?;

    params.inputs.push(Input {
        value_commit: revealed.value_commit,
        token_commit: revealed.token_commit,
        nullifier: revealed.nullifier,
        merkle_root: revealed.merkle_root,
        spend_hook: revealed.spend_hook,
        user_data_enc: revealed.user_data_enc,
        signature_public: revealed.signature_public,
    });

    zk_proofs.push(proof);

    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);

    // Disable composability for this old obsolete API
    let spend_hook = pallas::Base::zero();
    let user_data = pallas::Base::zero();

    info!(target: "money", "Creating swap mint proof for output 0");
    let (proof, revealed) = create_transfer_mint_proof(
        mint_zkbin,
        mint_pk,
        output.value,
        output.token_id,
        value_recv_blind,
        token_recv_blind,
        serial,
        spend_hook,
        user_data,
        coin_blind,
        output.public_key,
    )?;

    zk_proofs.push(proof);

    // Encrypted note
    let note = Note {
        serial,
        value: output.value,
        token_id: output.token_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind,
        value_blind: value_recv_blind,
        token_blind: token_recv_blind,
        // Here we store our secret key we use for signing
        memo: serialize(&signature_secret),
    };

    let encrypted_note = note.encrypt(&output.public_key)?;

    params.outputs.push(Output {
        value_commit: revealed.value_commit,
        token_commit: revealed.token_commit,
        coin: revealed.coin.inner(),
        ciphertext: encrypted_note.ciphertext,
        ephem_public: encrypted_note.ephem_public,
    });

    // Now we should have all the params, zk proofs, and signature secrets.
    // We return it all and let the caller deal with it.
    Ok((params, zk_proofs, vec![signature_secret], spent_coins, val_blinds, tok_blinds))
}

/// Build money contract transfer transaction parameters with the given data:
/// * `keypair` - Caller's keypair
/// * `pubkey` - Public key of the recipient
/// * `value` - Value of the transfer
/// * `token_id` - Token ID to transfer
/// * `spend_hook` - Spend hook
/// * `user_data` - User data
/// * `user_data_blind` - Blinding for user data
/// * `coins` - Set of coins we're able to spend
/// * `tree` - Current Merkle tree of coins
/// * `mint_zkbin` - ZkBinary of the mint circuit
/// * `mint_pk` - Proving key for the ZK mint proof
/// * `burn_zkbin` - ZkBinary of the burn circuit
/// * `burn_pk` - Proving key for the ZK burn proof
/// * `clear_input` - Marks if we're creating clear or anonymous inputs
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn build_transfer_tx(
    keypair: &Keypair,
    pubkey: &PublicKey,
    value: u64,
    token_id: TokenId,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    user_data_blind: pallas::Base,
    coins: &[OwnCoin],
    tree: &MerkleTree,
    mint_zkbin: &ZkBinary,
    mint_pk: &ProvingKey,
    burn_zkbin: &ZkBinary,
    burn_pk: &ProvingKey,
    clear_input: bool,
) -> Result<(MoneyTransferParams, Vec<Proof>, Vec<SecretKey>, Vec<OwnCoin>)> {
    debug!(target: "money", "Building money contract transfer transaction");
    assert!(value != 0);
    if !clear_input {
        assert!(!coins.is_empty());
    }
    // Ensure the coins given to us are all of the same token_id.
    // The money contract base transfer doesn't allow conversions.
    for coin in coins.iter() {
        assert_eq!(token_id, coin.note.token_id);
    }

    let mut clear_inputs = vec![];
    let mut inputs = vec![];
    let mut outputs = vec![];
    let mut change_outputs = vec![];
    let mut spent_coins = vec![];

    if clear_input {
        debug!(target: "money", "Money::build_transfer_tx(): Building clear input");
        let input =
            TransactionBuilderClearInputInfo { value, token_id, signature_secret: keypair.secret };
        clear_inputs.push(input);
    } else {
        debug!(target: "money", "Money::build_transfer_tx(): Building anonymous inputs");
        let mut inputs_value = 0;
        for coin in coins.iter() {
            if inputs_value >= value {
                debug!(target: "money", "inputs_value >= value");
                break
            }

            let leaf_position = coin.leaf_position;
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            inputs_value += coin.note.value;

            let input = TransactionBuilderInputInfo {
                leaf_position,
                merkle_path,
                secret: coin.secret,
                note: coin.note.clone(),
            };

            inputs.push(input);
            spent_coins.push(coin.clone());
        }

        if inputs_value < value {
            error!(target: "money", "Money::build_transfer_tx(): Not enough value to build tx inputs");
            return Err(ClientFailed::NotEnoughValue(inputs_value).into())
        }

        if inputs_value > value {
            let return_value = inputs_value - value;
            change_outputs.push(TransactionBuilderOutputInfo {
                value: return_value,
                token_id,
                public_key: keypair.public,
            });
        }

        debug!(target: "money", "Money::build_transfer_tx(): Finished building inputs");
    }

    outputs.push(TransactionBuilderOutputInfo { value, token_id, public_key: *pubkey });
    assert!(clear_inputs.len() + inputs.len() > 0);

    // We now fill this with necessary stuff
    let mut params = MoneyTransferParams { clear_inputs: vec![], inputs: vec![], outputs: vec![] };

    // I assumed this vec will contain a secret key for each clear input and anonymous input.
    let mut signature_secrets = vec![];

    let token_blind = ValueBlind::random(&mut OsRng);
    for input in clear_inputs {
        // TODO: FIXME: What to do with this signature secret?
        let signature_public = PublicKey::from_secret(input.signature_secret);
        signature_secrets.push(input.signature_secret);
        let value_blind = ValueBlind::random(&mut OsRng);

        params.clear_inputs.push(ClearInput {
            value: input.value,
            token_id: input.token_id,
            value_blind,
            token_blind,
            signature_public,
        });
    }

    let mut input_blinds = vec![];
    let mut output_blinds = vec![];
    let mut zk_proofs = vec![];

    for (i, input) in inputs.iter().enumerate() {
        let value_blind = ValueBlind::random(&mut OsRng);
        input_blinds.push(value_blind);

        let signature_secret = SecretKey::random(&mut OsRng);
        signature_secrets.push(signature_secret);

        info!(target: "money", "Creating transfer burn proof for input {}", i);
        let (proof, revealed) = create_transfer_burn_proof(
            burn_zkbin,
            burn_pk,
            input.note.value,
            input.note.token_id,
            value_blind,
            token_blind,
            input.note.serial,
            pallas::Base::zero(),
            pallas::Base::zero(),
            user_data_blind, // <-- FIXME: This api needs rework to support normal and DAO transfers
            input.note.coin_blind,
            input.secret,
            input.leaf_position,
            input.merkle_path.clone(),
            signature_secret,
        )?;

        params.inputs.push(Input {
            value_commit: revealed.value_commit,
            token_commit: revealed.token_commit,
            nullifier: revealed.nullifier,
            merkle_root: revealed.merkle_root,
            spend_hook: revealed.spend_hook,
            user_data_enc: revealed.user_data_enc,
            signature_public: revealed.signature_public,
        });

        zk_proofs.push(proof);
    }

    // This value_blind calc assumes there will always be at least a single output
    assert!(!outputs.is_empty());

    for (i, output) in change_outputs.iter().chain(outputs.iter()).enumerate() {
        let value_blind = if i == outputs.len() + change_outputs.len() - 1 {
            compute_remainder_blind(&params.clear_inputs, &input_blinds, &output_blinds)
        } else {
            ValueBlind::random(&mut OsRng)
        };

        output_blinds.push(value_blind);

        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);

        // A hacky way to zeroize spend hooks for the change outputs
        let (scoped_spend_hook, scoped_user_data) = {
            if i >= change_outputs.len() {
                (spend_hook, user_data)
            } else {
                (pallas::Base::zero(), pallas::Base::zero())
            }
        };

        info!(target: "money", "Creating transfer mint proof for output {}", i);
        let (proof, revealed) = create_transfer_mint_proof(
            mint_zkbin,
            mint_pk,
            output.value,
            output.token_id,
            value_blind,
            token_blind,
            serial,
            scoped_spend_hook,
            scoped_user_data,
            coin_blind,
            output.public_key,
        )?;

        zk_proofs.push(proof);

        // Encrypted note
        let note = Note {
            serial,
            value: output.value,
            token_id: output.token_id,
            spend_hook: scoped_spend_hook,
            user_data: scoped_user_data,
            coin_blind,
            value_blind,
            token_blind,
            // NOTE: Perhaps pass in memos to this entire function with
            //       VecDeque and then pop front to add here.
            memo: vec![],
        };

        let encrypted_note = note.encrypt(&output.public_key)?;

        params.outputs.push(Output {
            value_commit: revealed.value_commit,
            token_commit: revealed.token_commit,
            coin: revealed.coin.inner(),
            ciphertext: encrypted_note.ciphertext,
            ephem_public: encrypted_note.ephem_public,
        })
    }

    // Now we should have all the params, zk proofs, and signature secrets.
    // We return it all and let the caller deal with it.
    Ok((params, zk_proofs, signature_secrets, spent_coins))
}

pub fn build_stake_tx(
    //pubkey: &PublicKey,
    coins: &[OwnCoin],
    tx_tree: &mut MerkleTree,
    cm_tree: &mut MerkleTree,
    sk_tree: &mut MerkleTree,
    mint_zkbin: &ZkBinary,
    mint_pk: &ProvingKey,
    burn_zkbin: &ZkBinary,
    burn_pk: &ProvingKey,
    slot_index: u64,
) -> Result<(MoneyStakeParams, Vec<Proof>, Vec<LeadCoin>, Vec<ValueBlind>, Vec<ValueBlind>)> {
    // convert owncoins to leadcoins.
    // TODO: verify this token blind usage
    let token_blind = ValueBlind::random(&mut OsRng);
    let mut leadcoins: Vec<LeadCoin> = vec![];
    let mut params = MoneyStakeParams { inputs: vec![], outputs: vec![], token_blind };
    let mut proofs = vec![];
    let mut own_blinds = vec![];
    let mut lead_blinds = vec![];
    for coin in coins.iter() {
        // burn the coin
        let value_blind = ValueBlind::random(&mut OsRng);
        own_blinds.push(value_blind);
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();
        let user_data_blind = pallas::Base::random(&mut OsRng);
        let tx_leaf_position = coin.leaf_position;
        let tx_root = tx_tree.root(0).unwrap();
        let tx_merkle_path = tx_tree.authentication_path(tx_leaf_position, &tx_root).unwrap();
        let signature_secret = SecretKey::random(&mut OsRng);
        //signature_secrets.push(signature_secret);
        let (own_proof, own_revealed) = create_transfer_burn_proof(
            burn_zkbin,
            burn_pk,
            coin.note.value,
            coin.note.token_id,
            coin.note.value_blind,
            coin.note.token_blind,
            coin.note.serial,
            spend_hook,
            user_data,
            user_data_blind,
            coin.note.coin_blind,
            coin.secret,
            coin.leaf_position,
            tx_merkle_path.clone(),
            signature_secret,
        )?;
        params.inputs.push(Input {
            value_commit: own_revealed.value_commit,
            token_commit: own_revealed.token_commit,
            nullifier: own_revealed.nullifier,
            merkle_root: own_revealed.merkle_root,
            spend_hook: own_revealed.spend_hook,
            user_data_enc: own_revealed.user_data_enc,
            signature_public: own_revealed.signature_public,
        });
        proofs.push(own_proof);
        let lead_value_blind = ValueBlind::random(&mut OsRng);
        lead_blinds.push(lead_value_blind);
        sk_tree.append(&MerkleNode::from(coin.secret.inner()));
        let sk_pos = sk_tree.witness().unwrap();
        let sk_root = sk_tree.root(0).unwrap();
        let sk_merkle_path = sk_tree.authentication_path(sk_pos, &sk_root).unwrap();
        let leadcoin = LeadCoin::new(
            coin.note.value,
            slot_index,          // tau
            coin.secret.inner(), // coin secret key
            sk_root,
            sk_pos.try_into().unwrap(),
            sk_merkle_path,
            coin.note.serial,
            cm_tree,
        );
        leadcoins.push(leadcoin.clone());
        let lead_coin_blind = ValueBlind::random(&mut OsRng);
        let public_key = leadcoin.pk();
        let (lead_proof, lead_revealed) = create_stake_mint_proof(
            mint_zkbin,
            mint_pk,
            public_key,
            leadcoin.coin1_commitment,
            pallas::Base::from(coin.note.value),
            lead_value_blind,
            lead_coin_blind,
            coin.secret.inner(),
            sk_root.inner(),
            pallas::Base::from(slot_index), // tau
            coin.note.serial,               // nonce
        )?;
        let coin_commit_coords = [lead_revealed.commitment_x, lead_revealed.commitment_y];
        let coin_commit_hash = poseidon_hash(coin_commit_coords);
        params.outputs.push(StakedOutput {
            value_commit: lead_revealed.value_commit,
            coin_commit_hash,
            coin_pk_hash: public_key,
        });
        proofs.push(lead_proof);
    }
    Ok((params, proofs, leadcoins, own_blinds, lead_blinds))
}

pub fn build_unstake_tx(
    pubkey: &PublicKey, //recepient of owncoin public key
    token_id_recv: TokenId,
    coins: &[LeadCoin],
    mint_zkbin: &ZkBinary, // stake own mint binary
    mint_pk: &ProvingKey,
    burn_zkbin: &ZkBinary, // unstake lead burn binary
    burn_pk: &ProvingKey,
) -> Result<(MoneyUnstakeParams, Vec<Proof>, Vec<SecretKey>, Vec<ValueBlind>, Vec<ValueBlind>)> {
    // convert leadcoin to owncoin
    // TODO: verify this token blind usage
    let token_blind = ValueBlind::random(&mut OsRng);
    //let owncoins : Vec<OwnCoin>= vec![];
    let mut params = MoneyUnstakeParams { inputs: vec![], outputs: vec![], token_blind };
    let mut proofs = vec![];
    let mut own_blinds = vec![];
    let mut lead_blinds = vec![];
    for coin in coins.iter() {
        // burn lead coin
        let value_blind = ValueBlind::random(&mut OsRng);
        lead_blinds.push(value_blind);
        let pk = coin.pk();
        let nullifier = coin.sn();
        let (unstake_proof, unstake_revealed) = create_unstake_burn_proof(
            burn_zkbin,
            burn_pk,
            pallas::Base::from(coin.value),
            value_blind,
            coin.coin1_blind,
            pk,
            coin.coin1_sk,
            coin.coin1_sk_root.inner(),
            MerklePosition::from(coin.coin1_sk_pos as usize),
            coin.coin1_sk_merkle_path.to_vec(),
            coin.coin1_commitment_merkle_path.to_vec(),
            coin.coin1_commitment,
            coin.coin1_commitment_root.inner(),
            MerklePosition::from(coin.coin1_commitment_pos as usize),
            coin.slot,
            coin.nonce,
            nullifier,
        )?;
        let commitment_coord = [unstake_revealed.commitment_x, unstake_revealed.commitment_y];
        let coin_commitment_hash = poseidon_hash(commitment_coord);
        params.inputs.push(StakedInput {
            nullifier: nullifier.into(),
            value_commit: unstake_revealed.value_commit,
            coin_commit_hash: coin_commitment_hash,
            coin_pk_hash: unstake_revealed.pk,
            coin_commit_root: unstake_revealed.commitment_root.into(),
            sk_root: unstake_revealed.sk_root.into(),
        });
        proofs.push(unstake_proof);
        let own_value_blind = ValueBlind::random(&mut OsRng);
        own_blinds.push(own_value_blind);
        // mint own coin
        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);
        let token_recv_blind = ValueBlind::random(&mut OsRng);
        // Disable composability for this old obsolete API
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();
        let (proof, revealed) = create_transfer_mint_proof(
            mint_zkbin,
            mint_pk,
            coin.value,
            token_id_recv,
            own_value_blind,
            token_recv_blind,
            serial,
            spend_hook,
            user_data,
            coin_blind,
            *pubkey, //receipient public_key
        )?;
        proofs.push(proof);
        // Encrypted note
        let note = Note {
            serial,
            value: coin.value,
            token_id: token_id_recv,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind,
            value_blind,
            token_blind: token_recv_blind,
            // Here we store our secret key we use for signing
            memo: vec![],
        };

        let encrypted_note = note.encrypt(&pubkey)?;

        params.outputs.push(Output {
            value_commit: revealed.value_commit,
            token_commit: revealed.token_commit,
            coin: revealed.coin.inner(),
            ciphertext: encrypted_note.ciphertext,
            ephem_public: encrypted_note.ephem_public,
        });
    }
    Ok((params, proofs, vec![], lead_blinds, own_blinds))
}

fn compute_remainder_blind(
    clear_inputs: &[ClearInput],
    input_blinds: &[ValueBlind],
    output_blinds: &[ValueBlind],
) -> ValueBlind {
    let mut total = ValueBlind::zero();

    for input in clear_inputs {
        total += input.value_blind;
    }

    for input_blind in input_blinds {
        total += input_blind
    }

    for output_blind in output_blinds {
        total -= output_blind;
    }

    total
}

#[cfg(test)]
mod tests {
    use darkfi_sdk::pasta::group::ff::Field;

    use super::*;

    #[test]
    fn test_note_encdec() {
        let note = Note {
            serial: pallas::Base::random(&mut OsRng),
            value: 100,
            token_id: TokenId::from(pallas::Base::random(&mut OsRng)),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: pallas::Base::random(&mut OsRng),
            value_blind: pallas::Scalar::random(&mut OsRng),
            token_blind: pallas::Scalar::random(&mut OsRng),
            memo: vec![32, 223, 231, 3, 1, 1],
        };

        let keypair = Keypair::random(&mut OsRng);

        let encrypted_note = note.encrypt(&keypair.public).unwrap();
        let note2 = encrypted_note.decrypt(&keypair.secret).unwrap();
        assert_eq!(note.serial, note2.serial);
        assert_eq!(note.value, note2.value);
        assert_eq!(note.token_id, note2.token_id);
        assert_eq!(note.coin_blind, note2.coin_blind);
        assert_eq!(note.value_blind, note2.value_blind);
        assert_eq!(note.token_blind, note2.token_blind);
        assert_eq!(note.memo, note2.memo);
        assert_eq!(note, note2);
    }
}
