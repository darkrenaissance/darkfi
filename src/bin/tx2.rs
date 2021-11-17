use log::*;
use rand::rngs::OsRng;
use std::{fmt, time::Instant};

use halo2::{
    circuit::{Layouter, SimpleFloorPlanner},
    dev::MockProver,
    plonk::{
        Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn, Selector,
    },
    poly::Rotation,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPoints,
    },
    poseidon::{
        Hash as PoseidonHash, Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig,
        StateWord, Word,
    },
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::chip::{MerkleChip, MerkleConfig},
        merkle::MerklePath,
    },
    utilities::{
        copy, lookup_range_check::LookupRangeCheckConfig, CellValue, UtilitiesInstructions, Var,
    },
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field, FieldExt},
    group::{
        ff::{PrimeField, PrimeFieldBits},
        Curve,
    },
    pallas,
};
use incrementalmerkletree::{
    bridgetree::{BridgeTree, Frontier as BridgeFrontier},
    Altitude, Frontier, Tree,
};

use drk::{
    circuit::{mint_contract::MintContract, spend_contract::SpendContract},
    crypto::{
        coin::Coin,
        constants::{
            sinsemilla::{OrchardCommitDomains, OrchardHashDomains, MERKLE_CRH_PERSONALIZATION},
            OrchardFixedBases,
        },
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        proof::{Proof, ProvingKey, VerifyingKey},
        util::mod_r_p,
        util::{pedersen_commitment_scalar, pedersen_commitment_u64},
    },
};

struct MemoryState {
    mint_vk: VerifyingKey,
    spend_vk: VerifyingKey,
}

impl ProgramState for MemoryState {
    fn is_valid_cashier_public_key(&self, public: &pallas::Point) -> bool {
        true
    }

    fn mint_vk(&self) -> &VerifyingKey {
        &self.mint_vk
    }
    fn spend_vk(&self) -> &VerifyingKey {
        &self.spend_vk
    }
}

impl MemoryState {
    fn apply(&mut self, mut update: StateUpdate) {}
}

mod tx2 {
    use rand::rngs::OsRng;

    use pasta_curves::{
        arithmetic::{CurveAffine, Field},
        group::{
            ff::{PrimeField, PrimeFieldBits},
            Curve, Group,
        },
        pallas,
    };

    use super::{VerifyFailed, VerifyResult};
    use drk::{
        crypto::{
            mint_proof::{create_mint_proof, verify_mint_proof, MintRevealedValues},
            note::{EncryptedNote, Note},
            proof::{Proof, VerifyingKey},
            util::pedersen_commitment_u64,
        },
        error::Result,
        types::{derive_public_key, DrkCoinBlind, DrkSerial, DrkValueBlind, DrkValueCommit},
    };

    type DrkTokenId2 = u64;

    pub struct TransactionBuilder {
        pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
        pub inputs: Vec<TransactionBuilderInputInfo>,
        pub outputs: Vec<TransactionBuilderOutputInfo>,
    }

    pub struct TransactionBuilderClearInputInfo {
        pub value: u64,
        pub token_id: DrkTokenId2,
        pub signature_secret: pallas::Base,
    }

    pub struct TransactionBuilderInputInfo {}

    pub struct TransactionBuilderOutputInfo {
        pub value: u64,
        pub token_id: DrkTokenId2,
        pub public: pallas::Point,
    }

    impl TransactionBuilder {
        fn compute_remainder_blind(
            clear_inputs: &[PartialTransactionClearInput],
            input_blinds: &[DrkValueBlind],
            output_blinds: &[DrkValueBlind],
        ) -> DrkValueBlind {
            let mut total = DrkValueBlind::zero();

            for input in clear_inputs {
                total += input.value_blind;
            }

            for input_blind in input_blinds {
                total += input_blind;
            }

            for output_blind in output_blinds {
                total -= output_blind;
            }

            total
        }

        pub fn build(self) -> Result<Transaction> {
            let mut clear_inputs = vec![];
            let token_blind = DrkValueBlind::random(&mut OsRng);
            for input in &self.clear_inputs {
                let signature_public = derive_public_key(input.signature_secret);
                let value_blind = DrkValueBlind::random(&mut OsRng);

                let clear_input = PartialTransactionClearInput {
                    value: input.value,
                    token_id: input.token_id,
                    value_blind,
                    token_blind,
                    signature_public,
                };
                clear_inputs.push(clear_input);
            }

            let mut input_blinds = vec![];

            let mut outputs = vec![];
            let mut output_blinds = vec![];

            for (i, output) in self.outputs.iter().enumerate() {
                let value_blind = if i == self.outputs.len() - 1 {
                    Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds)
                } else {
                    DrkValueBlind::random(&mut OsRng)
                };
                output_blinds.push(value_blind);

                let serial = DrkSerial::random(&mut OsRng);
                let coin_blind = DrkCoinBlind::random(&mut OsRng);

                let (mint_proof, revealed) = create_mint_proof(
                    output.value,
                    pallas::Base::from(output.token_id),
                    value_blind,
                    token_blind,
                    serial,
                    coin_blind,
                    output.public,
                )?;

                // Encrypted note

                let note = Note {
                    serial,
                    value: output.value,
                    token_id: pallas::Base::from(output.token_id),
                    coin_blind,
                    value_blind,
                };

                let encrypted_note = note.encrypt(&output.public).unwrap();

                let output = TransactionOutput {
                    mint_proof,
                    revealed,
                    enc_note: encrypted_note,
                };
                outputs.push(output);
            }

            let partial_tx = PartialTransaction {
                clear_inputs,
                //inputs,
                outputs,
            };

            let mut clear_inputs = vec![];
            for (input, info) in partial_tx.clear_inputs.into_iter().zip(self.clear_inputs) {
                //let secret = schnorr::SecretKey(info.signature_secret);
                //let signature = secret.sign(&unsigned_tx_data[..]);
                let input = TransactionClearInput::from_partial(input);
                clear_inputs.push(input);
            }

            Ok(Transaction {
                clear_inputs,
                outputs: partial_tx.outputs,
            })
        }
    }

    pub struct PartialTransaction {
        pub clear_inputs: Vec<PartialTransactionClearInput>,
        //pub inputs: Vec<PartialTransactionInput>,
        pub outputs: Vec<TransactionOutput>,
    }

    pub struct PartialTransactionClearInput {
        pub value: u64,
        pub token_id: DrkTokenId2,
        pub value_blind: DrkValueBlind,
        pub token_blind: DrkValueBlind,
        pub signature_public: pallas::Point,
    }

    pub struct Transaction {
        pub clear_inputs: Vec<TransactionClearInput>,
        pub outputs: Vec<TransactionOutput>,
    }

    impl Transaction {
        fn verify_token_commitments(&self) -> bool {
            assert_ne!(self.outputs.len(), 0);
            let token_commit_value = self.outputs[0].revealed.token_commit;

            let mut failed = self
                .outputs
                .iter()
                .any(|output| output.revealed.token_commit != token_commit_value);
            failed = failed
                || self.clear_inputs.iter().any(|input| {
                    pedersen_commitment_u64(input.token_id, input.token_blind) != token_commit_value
                });
            !failed
        }

        pub fn verify(
            &self,
            mint_vk: &VerifyingKey,
            spend_vk: &VerifyingKey,
        ) -> VerifyResult<()> {
            let mut valcom_total = DrkValueCommit::identity();

            for input in &self.clear_inputs {
                valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
            }

            //for (i, input) in self.inputs.iter().enumerate() {
            //    if verify_spend_proof(spend_pvk, input.spend_proof.clone(), &input.revealed).is_err() {
            //        return Err(VerifyFailed::SpendProof(i));
            //    }
            //    valcom_total += &input.revealed.value_commit;
            //}

            for (i, output) in self.outputs.iter().enumerate() {
                if verify_mint_proof(mint_vk, &output.mint_proof, &output.revealed).is_err()
                {
                    return Err(VerifyFailed::MintProof(i));
                }
                valcom_total -= &output.revealed.value_commit;
            }

            if valcom_total != DrkValueCommit::identity() {
                return Err(VerifyFailed::MissingFunds);
            }

            // Verify token commitments match
            if !self.verify_token_commitments() {
                return Err(VerifyFailed::TokenMismatch);
            }

            Ok(())
        }
    }

    pub struct TransactionClearInput {
        pub value: u64,
        pub token_id: DrkTokenId2,
        pub value_blind: DrkValueBlind,
        pub token_blind: DrkValueBlind,
        pub signature_public: pallas::Point,
        //pub signature: schnorr::Signature,
    }

    impl TransactionClearInput {
        fn from_partial(partial: PartialTransactionClearInput) -> Self {
            Self {
                value: partial.value,
                token_id: partial.token_id,
                value_blind: partial.value_blind,
                token_blind: partial.token_blind,
                signature_public: partial.signature_public,
                //signature,
            }
        }
    }

    pub struct TransactionOutput {
        pub mint_proof: Proof,
        pub revealed: MintRevealedValues,
        pub enc_note: EncryptedNote,
    }
}

pub trait ProgramState {
    fn is_valid_cashier_public_key(&self, public: &pallas::Point) -> bool;
    //// TODO: fn is_valid_merkle(&self, merkle: &MerkleNode) -> bool;
    //fn nullifier_exists(&self, nullifier: &Nullifier) -> bool;

    fn mint_vk(&self) -> &VerifyingKey;
    fn spend_vk(&self) -> &VerifyingKey;
}

pub struct StateUpdate {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
    pub enc_notes: Vec<EncryptedNote>,
}

pub type VerifyResult<T> = std::result::Result<T, VerifyFailed>;

#[derive(Debug)]
pub enum VerifyFailed {
    InvalidCashierKey(usize),
    InvalidMerkle(usize),
    DuplicateNullifier(usize),
    SpendProof(usize),
    MintProof(usize),
    ClearInputSignature(usize),
    InputSignature(usize),
    MissingFunds,
    TokenMismatch,
}

impl std::error::Error for VerifyFailed {}

impl fmt::Display for VerifyFailed {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match *self {
            VerifyFailed::InvalidCashierKey(i) => {
                write!(f, "Invalid cashier public key for clear input {}", i)
            }
            VerifyFailed::InvalidMerkle(i) => {
                write!(f, "Invalid merkle root for input {}", i)
            }
            VerifyFailed::DuplicateNullifier(i) => {
                write!(f, "Duplicate nullifier for input {}", i)
            }
            VerifyFailed::SpendProof(i) => write!(f, "Spend proof for input {}", i),
            VerifyFailed::MintProof(i) => write!(f, "Mint proof for input {}", i),
            VerifyFailed::ClearInputSignature(i) => {
                write!(f, "Invalid signature for clear input {}", i)
            }
            VerifyFailed::InputSignature(i) => write!(f, "Invalid signature for input {}", i),
            VerifyFailed::MissingFunds => {
                f.write_str("Money in does not match money out (value commits)")
            }
            VerifyFailed::TokenMismatch => {
                f.write_str("Assets don't match some inputs or outputs (token commits)")
            }
        }
    }
}

pub fn state_transition<S: ProgramState>(
    state: &S,
    tx: tx2::Transaction,
) -> VerifyResult<StateUpdate> {
    // Check deposits are legit

    debug!(target: "STATE TRANSITION", "iterate clear_inputs");

    for (i, input) in tx.clear_inputs.iter().enumerate() {
        // Check the public key in the clear inputs
        // It should be a valid public key for the cashier

        if !state.is_valid_cashier_public_key(&input.signature_public) {
            log::error!(target: "STATE TRANSITION", "Not valid cashier public key");
            return Err(VerifyFailed::InvalidCashierKey(i));
        }
    }

    debug!(target: "STATE TRANSITION", "Check the tx Verifies correctly");
    // Check the tx verifies correctly
    tx.verify(state.mint_vk(), state.spend_vk())?;

    let mut nullifiers = vec![];

    // Newly created coins for this tx
    let mut coins = vec![];
    let mut enc_notes = vec![];
    for output in tx.outputs {
        // Gather all the coins
        coins.push(Coin::from_bytes(&output.revealed.coin));
        enc_notes.push(output.enc_note);
    }

    Ok(StateUpdate {
        nullifiers,
        coins,
        enc_notes,
    })
}

use std::iter;
use subtle::ConstantTimeEq;
use incrementalmerkletree::Hashable;
use halo2_gadgets::primitives::sinsemilla::HashDomain;
use lazy_static::lazy_static;

use drk::crypto::constants::{L_ORCHARD_MERKLE, MERKLE_DEPTH_ORCHARD, sinsemilla::i2lebsp_k};

//const UNCOMMITTED_ORCHARD: pallas::Base = pallas::Base::from_u64(2);

lazy_static! {
    static ref UNCOMMITTED_ORCHARD: pallas::Base = pallas::Base::from_u64(2);
    static ref EMPTY_ROOTS: Vec<MerkleNode> = {
        iter::empty()
            .chain(Some(MerkleNode::empty_leaf()))
            .chain(
                (0..MERKLE_DEPTH_ORCHARD).scan(MerkleNode::empty_leaf(), |state, l| {
                    let l = l as u8;
                    *state = MerkleNode::combine(l.into(), state, state);
                    Some(state.clone())
                }),
            )
            .collect()
    };
}

#[derive(Clone, std::cmp::Eq)]
pub struct MerkleNode(pallas::Base);

impl std::cmp::PartialEq for MerkleNode {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl std::hash::Hash for MerkleNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        <Option<pallas::Base>>::from(self.0)
            .map(|b| b.to_bytes())
            .hash(state)
    }
}

impl Hashable for MerkleNode {
    fn empty_leaf() -> Self {
        MerkleNode(UNCOMMITTED_ORCHARD.clone())
    }

    /// Implements `MerkleCRH^Orchard` as defined in
    /// <https://zips.z.cash/protocol/protocol.pdf#orchardmerklecrh>
    ///
    /// The layer with 2^n nodes is called "layer n":
    ///      - leaves are at layer MERKLE_DEPTH_ORCHARD = 32;
    ///      - the root is at layer 0.
    /// `l` is MERKLE_DEPTH_ORCHARD - layer - 1.
    ///      - when hashing two leaves, we produce a node on the layer above the leaves, i.e.
    ///        layer = 31, l = 0
    ///      - when hashing to the final root, we produce the anchor with layer = 0, l = 31.
    fn combine(altitude: Altitude, left: &Self, right: &Self) -> Self {
        // MerkleCRH Sinsemilla hash domain.
        let domain = HashDomain::new(MERKLE_CRH_PERSONALIZATION);

        MerkleNode(
            domain
                .hash(
                    iter::empty()
                        .chain(i2lebsp_k(altitude.into()).iter().copied())
                        .chain(left.0.to_le_bits().iter().by_val().take(L_ORCHARD_MERKLE))
                        .chain(right.0.to_le_bits().iter().by_val().take(L_ORCHARD_MERKLE)),
                )
                .unwrap_or(pallas::Base::zero()),
        )
    }

    fn empty_root(altitude: Altitude) -> Self {
        EMPTY_ROOTS[<usize>::from(altitude)].clone()
    }
}

fn main() -> std::result::Result<(), failure::Error> {
    use drk::{
        crypto::mint_proof::{create_mint_proof, verify_mint_proof},
        types::{DrkSerial, DrkCoinBlind, DrkCircuitField}
    };

    let cashier_secret = pallas::Base::random(&mut OsRng);
    let cashier_public = OrchardFixedBases::SpendAuthG.generator() * mod_r_p(cashier_secret);

    let secret = pallas::Base::random(&mut OsRng);
    let public = OrchardFixedBases::SpendAuthG.generator() * mod_r_p(secret);

    const K: u32 = 11;
    let mint_vk = VerifyingKey::build(K, MintContract::default());
    let spend_vk = VerifyingKey::build(K, SpendContract::default());

    let mut state = MemoryState { mint_vk, spend_vk };

    let token_id = 110;

    let builder = tx2::TransactionBuilder {
        clear_inputs: vec![tx2::TransactionBuilderClearInputInfo {
            value: 110,
            token_id,
            signature_secret: cashier_secret,
        }],
        inputs: vec![],
        outputs: vec![tx2::TransactionBuilderOutputInfo {
            value: 110,
            token_id,
            public,
        }],
    };

    let tx = builder.build()?;

    let update = state_transition(&state, tx)?;
    state.apply(update);

    //let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    //let coin = MerkleNode(pallas::Base::random(&mut OsRng));
    //tree.append(&coin);
    //tree.witness();

    let mut tree = BridgeTree::<String, 3>::new(100);

    Ok(())
}
