use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use group::Group;
use rand::rngs::OsRng;

use sapvi::crypto::{
    create_mint_proof, load_params, save_params, setup_mint_prover, verify_mint_proof,
    MintRevealedValues,
    note::Note
};

struct TransactionBuilder {
    clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    outputs: Vec<TransactionBuilderOutputInfo>,
}

impl TransactionBuilder {
    fn compute_remainder_blind(
        clear_inputs: &Vec<TransactionClearInput>,
        output_blinds: &Vec<jubjub::Fr>,
    ) -> jubjub::Fr {
        let mut lhs_total = jubjub::Fr::zero();
        for input in clear_inputs {
            lhs_total += input.valcom_blind;
        }

        let mut rhs_total = jubjub::Fr::zero();
        for output_blind in output_blinds {
            rhs_total += output_blind;
        }

        lhs_total - rhs_total
    }

    fn build(self, mint_params: &groth16::Parameters<Bls12>) -> Transaction {
        let mut clear_inputs = vec![];
        for input in &self.clear_inputs {
            let valcom_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let clear_input = TransactionClearInput {
                value: input.value,
                valcom_blind,
            };
            clear_inputs.push(clear_input);
        }

        let mut outputs = vec![];
        let mut output_blinds = vec![];
        for (i, output) in self.outputs.iter().enumerate() {
            let valcom_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&clear_inputs, &output_blinds)
            } else {
                jubjub::Fr::random(&mut OsRng)
            };
            output_blinds.push(valcom_blind);

            let serial: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let coin_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

            let (mint_proof, revealed) = create_mint_proof(
                mint_params,
                output.value,
                valcom_blind,
                serial,
                coin_blind,
                output.public,
            );
            let output = TransactionOutput {
                mint_proof,
                revealed,
            };
            outputs.push(output);
        }

        Transaction {
            clear_inputs,
            outputs,
        }
    }
}

struct TransactionBuilderClearInputInfo {
    value: u64,
}

struct TransactionBuilderOutputInfo {
    value: u64,
    public: jubjub::SubgroupPoint,
}

struct Transaction {
    clear_inputs: Vec<TransactionClearInput>,
    outputs: Vec<TransactionOutput>,
}

impl Transaction {
    fn compute_value_commit(value: u64, blind: &jubjub::Fr) -> jubjub::SubgroupPoint {
        let value_commit = (zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR
            * jubjub::Fr::from(value))
            + (zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR * blind);
        value_commit
    }

    fn verify(&self, pvk: &groth16::PreparedVerifyingKey<Bls12>) -> bool {
        let mut valcom_total = jubjub::SubgroupPoint::identity();
        for input in &self.clear_inputs {
            valcom_total += Self::compute_value_commit(input.value, &input.valcom_blind);
        }
        for output in &self.outputs {
            if !verify_mint_proof(pvk, &output.mint_proof, &output.revealed) {
                return false;
            }
            valcom_total -= &output.revealed.value_commit;
        }

        valcom_total == jubjub::SubgroupPoint::identity()
    }
}

struct TransactionClearInput {
    value: u64,
    valcom_blind: jubjub::Fr,
}

struct TransactionOutput {
    mint_proof: groth16::Proof<Bls12>,
    revealed: MintRevealedValues,
}

fn txbuilding() {
    {
        let params = setup_mint_prover();
        save_params("mint.params", &params);
    }
    let (mint_params, mint_pvk) = load_params("mint.params").expect("params should load");

    let public = jubjub::SubgroupPoint::random(&mut OsRng);

    let builder = TransactionBuilder {
        clear_inputs: vec![TransactionBuilderClearInputInfo { value: 110 }],
        outputs: vec![TransactionBuilderOutputInfo { value: 110, public }],
    };

    let tx = builder.build(&mint_params);
    assert!(tx.verify(&mint_pvk));
}

fn main() {
    // txbuilding()
    let note = Note {
        serial: jubjub::Fr::random(&mut OsRng),
        value: 110,
        coin_blind: jubjub::Fr::random(&mut OsRng),
        valcom_blind: jubjub::Fr::random(&mut OsRng),
    };

    let secret = jubjub::Fr::random(&mut OsRng);
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let encrypted_note = note.encrypt(&public).unwrap();
    let note2 = encrypted_note.decrypt(&secret).unwrap();
    assert_eq!(note.value, note2.value);
}
