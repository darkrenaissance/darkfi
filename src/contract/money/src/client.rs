/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
    crypto::proof::{Proof, ProvingKey},
    zk::{vm::ZkCircuit, vm_stack::Witness},
    zkas::ZkBinary,
    ClientFailed, Error, Result,
};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH,
        diffie_hellman::{kdf_sapling, sapling_ka_agree},
        pedersen::{pedersen_commitment_base, pedersen_commitment_u64, ValueBlind, ValueCommit},
        poseidon_hash, Keypair, MerkleNode, Nullifier, PublicKey, SecretKey, TokenId,
    },
    incrementalmerkletree,
    incrementalmerkletree::{bridgetree::BridgeTree, Hashable, Tree},
    pasta::{
        arithmetic::CurveAffine,
        group::{ff::PrimeField, Curve},
        pallas,
    },
};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use log::{debug, error, info};
use rand::rngs::OsRng;

use crate::state::{ClearInput, Input, MoneyTransferParams, Output};

// Wallet SQL table constant names
pub const MONEY_TREE_TABLE: &str = "money_tree";
pub const MONEY_TREE_COL_TREE: &str = "tree";

/// Byte length of the AEAD tag of the chacha20 cipher used for note encryption
pub const AEAD_TAG_SIZE: usize = 16;

/// The `Coin` is represented as a base field element.
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Coin(pallas::Base);

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
    pub leaf_position: incrementalmerkletree::Position,
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

struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub signature_secret: SecretKey,
}

struct TransactionBuilderInputInfo {
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: Note,
}

struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub public_key: PublicKey,
}

struct TransferBurnRevealed {
    pub value_commit: ValueCommit,
    pub token_commit: ValueCommit,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

impl TransferBurnRevealed {
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
        leaf_position: incrementalmerkletree::Position,
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

struct TransferMintRevealed {
    pub coin: Coin,
    pub value_commit: ValueCommit,
    pub token_commit: ValueCommit,
}

impl TransferMintRevealed {
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

fn create_transfer_mint_proof(
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

fn create_transfer_burn_proof(
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
    leaf_position: incrementalmerkletree::Position,
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

/// Build money contract transfer transaction parameters with the given data:
/// * `keypair` - Caller's keypair
/// * `pubkey` - Public key of the recipient
/// * `value` - Value of the transfer
/// * `token_id` - Token id of the Coin
/// * `coins` - Set of coins we're able to spend
/// * `tree` - Current Merkle tree of coins
/// * `mint_zkbin` - Mint ZK binary
/// * `mint_pk` - Proving key for the ZK mint proof
/// * `burn_zkbin` - Burn ZK binary
/// * `burn_pk` - Proving key for the ZK burn proof
/// * `clear_input` - Marks if we're creating clear or anonymous inputs
pub fn build_transfer_tx(
    keypair: &Keypair,
    pubkey: &PublicKey,
    value: u64,
    token_id: TokenId,
    coins: &[OwnCoin],
    tree: &BridgeTree<MerkleNode, MERKLE_DEPTH>,
    mint_zkbin: &ZkBinary,
    mint_pk: &ProvingKey,
    burn_zkbin: &ZkBinary,
    burn_pk: &ProvingKey,
    clear_input: bool,
) -> Result<(MoneyTransferParams, Vec<Proof>, Vec<SecretKey>)> {
    debug!("Building money contract transaction");
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
    let mut spent_coins = vec![];

    if clear_input {
        debug!("Money::build_transfer_tx(): Building clear input");
        let input =
            TransactionBuilderClearInputInfo { value, token_id, signature_secret: keypair.secret };
        clear_inputs.push(input);

        // HERE WE ARE NOT CHECKING FOR inputs_value >= value & ADDING SPENT_COINS ??
    } else {
        debug!("Money::build_transfer_tx(): Building anonymous inputs");
        let mut inputs_value = 0;
        for coin in coins.iter() {
            if inputs_value >= value {
                debug!("inputs_value >= value");
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
            spent_coins.push(coin);
        }

        if inputs_value < value {
            error!("Money::build_transfer_tx(): Not enough value to build tx inputs");
            return Err(ClientFailed::NotEnoughValue(inputs_value).into())
        }

        // add the leftover value to outputs inorder to mint us a new coin 
        if inputs_value > value {
            let return_value = inputs_value - value;
            outputs.push(TransactionBuilderOutputInfo {
                value: return_value,
                token_id,
                public_key: keypair.public,
            });
        }

        debug!("Money::build_transfer_tx(): Finished building inputs");
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

        // Disable composability for this old obsolete API
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();
        let user_data_blind = pallas::Base::random(&mut OsRng);

        info!("Creating transfer burn proof for input {}", i);
        let (proof, revealed) = create_transfer_burn_proof(
            burn_zkbin,
            burn_pk,
            input.note.value,
            input.note.token_id,
            value_blind,
            token_blind,
            input.note.serial,
            spend_hook,
            user_data,
            user_data_blind,
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

    for (i, output) in outputs.iter().enumerate() {
        let value_blind = if i == outputs.len() - 1 {
            compute_remainder_blind(&params.clear_inputs, &input_blinds, &output_blinds)
        } else {
            ValueBlind::random(&mut OsRng)
        };

        output_blinds.push(value_blind);

        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);

        // Disable composability for this old obsolete API
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();

        info!("Creating transfer mint proof for output {}", i);
        let (proof, revealed) = create_transfer_mint_proof(
            mint_zkbin,
            mint_pk,
            output.value,
            output.token_id,
            value_blind,
            token_blind,
            serial,
            spend_hook,
            user_data,
            coin_blind,
            output.public_key,
        )?;

        zk_proofs.push(proof);

        let note = Note {
            serial,
            value: output.value,
            token_id: output.token_id,
            coin_blind,
            value_blind,
            token_blind,
            // NOTE: Perhaps pass in memos to this entire function with
            //       VecDeque and then pop front to add here.
            memo: vec![],
        };
        // Encrypted note
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
    // TODO: Return also spent coins

    Ok((params, zk_proofs, signature_secrets))
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
