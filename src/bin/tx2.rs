use rand::rngs::OsRng;
use std::{fmt, time::Instant};
use log::*;

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
    arithmetic::{CurveAffine, Field},
    group::{ff::{PrimeField, PrimeFieldBits}, Curve},
    pallas,
};

use drk::{
    crypto::{
        coin::Coin,
        constants::{
            sinsemilla::{OrchardCommitDomains, OrchardHashDomains, MERKLE_CRH_PERSONALIZATION},
            OrchardFixedBases,
        },
        nullifier::Nullifier,
        util::{
            pedersen_commitment_u64,
            pedersen_commitment_scalar
        },
        proof::{Proof, ProvingKey, VerifyingKey},
        util::mod_r_p,
    },
};

struct MemoryState {
}

impl ProgramState for MemoryState {
    fn is_valid_cashier_public_key(&self, public: &pallas::Point) -> bool {
        true
    }
}

impl MemoryState {
    fn apply(&mut self, mut update: StateUpdate) {
    }
}

mod tx2 {
    use pasta_curves::{
        arithmetic::{CurveAffine, Field},
        group::{ff::{PrimeField, PrimeFieldBits}, Curve},
        pallas,
    };

    use drk::types::derive_public_key;

    pub struct TransactionBuilder {
        pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
        pub inputs: Vec<TransactionBuilderInputInfo>,
        pub outputs: Vec<TransactionBuilderOutputInfo>
    }

    pub struct TransactionBuilderClearInputInfo {
        pub value: u64,
        pub token_id: u64,
        pub signature_secret: pallas::Base
    }

    pub struct TransactionBuilderInputInfo {
    }

    pub struct TransactionBuilderOutputInfo {
        pub value: u64,
        pub token_id: u64,
        pub public: pallas::Point
    }

    impl TransactionBuilder {
        pub fn build(self) -> Transaction {
            let mut clear_inputs = vec![];
            //let token_blind = DrkValueBlind::random(&mut OsRng);
            for input in &self.clear_inputs {
                let signature_public = derive_public_key(input.signature_secret);
                //let value_blind = DrkValueBlind::random(&mut OsRng);

                let clear_input = PartialTransactionClearInput {
                    value: input.value,
                    //token_id: input.token_id,
                    //value_blind,
                    //token_blind,
                    signature_public,
                };
                clear_inputs.push(clear_input);
            }

            let partial_tx = PartialTransaction {
                clear_inputs,
                //inputs,
                //outputs,
            };

            let mut clear_inputs = vec![];
            for (input, info) in partial_tx.clear_inputs.into_iter().zip(self.clear_inputs) {
                //let secret = schnorr::SecretKey(info.signature_secret);
                //let signature = secret.sign(&unsigned_tx_data[..]);
                let input = TransactionClearInput::from_partial(input);
                clear_inputs.push(input);
            }

            Transaction {
                clear_inputs
            }
        }
    }

    pub struct PartialTransaction {
        pub clear_inputs: Vec<PartialTransactionClearInput>,
        //pub inputs: Vec<PartialTransactionInput>,
        //pub outputs: Vec<TransactionOutput>,
    }
    
    pub struct PartialTransactionClearInput {
        pub value: u64,
        //pub token_id: DrkTokenId,
        //pub value_blind: DrkValueBlind,
        //pub token_blind: DrkValueBlind,
        pub signature_public: pallas::Point,
    }

    pub struct Transaction {
        pub clear_inputs: Vec<TransactionClearInput>,
    }

    pub struct TransactionClearInput {
        pub value: u64,
        //pub token_id: DrkTokenId,
        //pub value_blind: DrkValueBlind,
        //pub token_blind: DrkValueBlind,
        pub signature_public: pallas::Point,
        //pub signature: schnorr::Signature,
    }

    impl TransactionClearInput {
        fn from_partial(
            partial: PartialTransactionClearInput,
        ) -> Self {
            Self {
                value: partial.value,
                //token_id: partial.token_id,
                //value_blind: partial.value_blind,
                //token_blind: partial.token_blind,
                signature_public: partial.signature_public,
                //signature,
            }
        }
    }
}

pub trait ProgramState {
    fn is_valid_cashier_public_key(&self, public: &pallas::Point) -> bool;
    //// TODO: fn is_valid_merkle(&self, merkle: &MerkleNode) -> bool;
    //fn nullifier_exists(&self, nullifier: &Nullifier) -> bool;

    //fn mint_pvk(&self) -> &VerifyingKey;
    //fn spend_pvk(&self) -> &VerifyingKey;
}

pub struct StateUpdate {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
    //pub enc_notes: Vec<EncryptedNote>,
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
    AssetMismatch,
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
            VerifyFailed::AssetMismatch => {
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

    let mut nullifiers = vec![];

    // Newly created coins for this tx
    let mut coins = vec![];

    Ok(StateUpdate {
        nullifiers,
        coins,
    })
}

fn main() -> std::result::Result<(), failure::Error> {
    let cashier_secret = pallas::Base::random(&mut OsRng);
    let cashier_public = OrchardFixedBases::SpendAuthG.generator() * mod_r_p(cashier_secret);

    let secret = pallas::Base::random(&mut OsRng);
    let public = OrchardFixedBases::SpendAuthG.generator() * mod_r_p(secret);

    let mut state = MemoryState {
    };

    let token_id = 110;

    let builder = tx2::TransactionBuilder {
        clear_inputs: vec![tx2::TransactionBuilderClearInputInfo {
            value: 110,
            token_id,
            signature_secret: cashier_secret
        }],
        inputs: vec![],
        outputs: vec![tx2::TransactionBuilderOutputInfo {
            value: 110,
            token_id,
            public
        }]
    };

    let tx = builder.build();

    let update = state_transition(&state, tx)?;
    state.apply(update);

    Ok(())
}

