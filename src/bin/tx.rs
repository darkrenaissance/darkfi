use rand::rngs::OsRng;
use ff::Field;
use bellman::groth16;
use bls12_381::Bls12;

use sapvi::crypto::{save_params, load_params, setup_mint_prover, create_mint_proof, verify_mint_proof, MintRevealedValues};

struct TransactionBuilder {
    clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    outputs: Vec<TransactionBuilderOutputInfo>
}

impl TransactionBuilder {
    fn compute_remainder_blind(clear_inputs: &Vec<TransactionClearInput>, output_blinds: &Vec<jubjub::Fr>) -> jubjub::Fr {
        let mut lhs_total = jubjub::Fr::zero();
        for input in clear_inputs {
            lhs_total += input.valcom_blind;
        }

        let mut rhs_total = jubjub::Fr::zero();
        for output_blind in output_blinds {
            rhs_total += output_blind;
        }

        rhs_total - lhs_total
    }

    fn build(self, mint_params: &groth16::Parameters<Bls12>) -> Transaction {
        let mut clear_inputs = vec![];
        for input in &self.clear_inputs {
            let valcom_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let clear_input = TransactionClearInput {
                value: input.value,
                valcom_blind
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

            let (mint_proof, revealed) = create_mint_proof(mint_params, output.value, valcom_blind, serial, coin_blind, output.public);
            let output = TransactionOutput {
                mint_proof,
                revealed
            };
            outputs.push(output);
        }

        Transaction {
            clear_inputs,
            outputs
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

impl TransactionBuilderOutputInfo {
    fn build() {
    }
}

struct Transaction {
    clear_inputs: Vec<TransactionClearInput>,
    outputs: Vec<TransactionOutput>
}

impl Transaction {
    fn verify(&self, pvk: &groth16::PreparedVerifyingKey<Bls12>) -> bool {
        for input in &self.clear_inputs {
        }
        for output in &self.outputs {
            if !verify_mint_proof(pvk, &output.mint_proof, &output.revealed) {
                return false;
            }
        }
        true
    }
}

struct TransactionClearInput {
    value: u64,
    valcom_blind: jubjub::Fr
}

struct TransactionOutput {
    mint_proof: groth16::Proof<Bls12>,
    revealed: MintRevealedValues,
}

fn main() {
    {
        let params = setup_mint_prover();
        save_params("mint.params", &params);
    }
    let (mint_params, mint_pvk) = load_params("mint.params").expect("params should load");

    let builder = TransactionBuilder {
        clear_inputs: vec![],
        outputs: vec![]
    };

    let tx = builder.build(&mint_params);
    assert!(tx.verify(&mint_pvk));
}

