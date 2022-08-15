use pasta_curves::group::ff::Field;
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        burn_proof::create_burn_proof,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        mint_proof::create_mint_proof,
        note::Note,
        proof::ProvingKey,
        schnorr::SchnorrSecret,
        types::{DrkCoinBlind, DrkSerial, DrkTokenId, DrkValueBlind},
    },
    util::serial::Encodable,
    Result,
};

use super::partial::{Partial, PartialClearInput, PartialInput};
use crate::{
    demo::FuncCall,
    money_contract::transfer::validate::{CallData, ClearInput, Input, Output},
    ZkBinaryTable, ZkContractInfo,
};

pub struct Builder {
    pub clear_inputs: Vec<BuilderClearInputInfo>,
    pub inputs: Vec<BuilderInputInfo>,
    pub outputs: Vec<BuilderOutputInfo>,
}

pub struct BuilderClearInputInfo {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub signature_secret: SecretKey,
}

pub struct BuilderInputInfo {
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: Note,
}

pub struct BuilderOutputInfo {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub public: PublicKey,
}

impl Builder {
    fn compute_remainder_blind(
        clear_inputs: &[PartialClearInput],
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

    pub fn build(self, zk_bins: &ZkBinaryTable) -> Result<FuncCall> {
        assert!(self.clear_inputs.len() + self.inputs.len() > 0);

        let mut clear_inputs = vec![];
        let token_blind = DrkValueBlind::random(&mut OsRng);
        for input in &self.clear_inputs {
            let signature_public = PublicKey::from_secret(input.signature_secret);
            let value_blind = DrkValueBlind::random(&mut OsRng);

            let clear_input = PartialClearInput {
                value: input.value,
                token_id: input.token_id,
                value_blind,
                token_blind,
                signature_public,
            };
            clear_inputs.push(clear_input);
        }

        let mut proofs = vec![];

        let mut inputs = vec![];
        let mut input_blinds = vec![];
        let mut signature_secrets = vec![];
        for input in self.inputs {
            let value_blind = DrkValueBlind::random(&mut OsRng);
            input_blinds.push(value_blind);

            let signature_secret = SecretKey::random(&mut OsRng);

            let zk_info = zk_bins.lookup(&"money-transfer-burn".to_string()).unwrap();
            let zk_info = if let ZkContractInfo::Native(info) = zk_info {
                info
            } else {
                panic!("Not native info")
            };
            let burn_pk = &zk_info.proving_key;

            let (burn_proof, revealed) = create_burn_proof(
                burn_pk,
                input.note.value,
                input.note.token_id,
                value_blind,
                token_blind,
                input.note.serial,
                input.note.coin_blind,
                input.secret,
                input.leaf_position,
                input.merkle_path,
                signature_secret,
            )?;
            proofs.push(burn_proof);

            // First we make the tx then sign after
            signature_secrets.push(signature_secret);

            let input = PartialInput { revealed };
            inputs.push(input);
        }

        let mut outputs = vec![];
        let mut output_blinds = vec![];
        // This value_blind calc assumes there will always be at least a single output
        assert!(self.outputs.len() > 0);

        for (i, output) in self.outputs.iter().enumerate() {
            let value_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds)
            } else {
                DrkValueBlind::random(&mut OsRng)
            };
            output_blinds.push(value_blind);

            let serial = DrkSerial::random(&mut OsRng);
            let coin_blind = DrkCoinBlind::random(&mut OsRng);

            let zk_info = zk_bins.lookup(&"money-transfer-mint".to_string()).unwrap();
            let zk_info = if let ZkContractInfo::Native(info) = zk_info {
                info
            } else {
                panic!("Not native info")
            };
            let mint_pk = &zk_info.proving_key;

            let (mint_proof, revealed) = create_mint_proof(
                mint_pk,
                output.value,
                output.token_id,
                value_blind,
                token_blind,
                serial,
                coin_blind,
                output.public,
            )?;
            proofs.push(mint_proof);

            // Encrypted note
            let note = Note {
                serial,
                value: output.value,
                token_id: output.token_id,
                coin_blind,
                value_blind,
                token_blind,
                memo: vec![],
            };

            let encrypted_note = note.encrypt(&output.public)?;

            let output = Output { revealed, enc_note: encrypted_note };
            outputs.push(output);
        }

        let partial = Partial { clear_inputs, inputs, outputs, proofs };

        let mut unsigned_tx_data = vec![];
        partial.encode(&mut unsigned_tx_data)?;

        let mut clear_inputs = vec![];
        for (input, info) in partial.clear_inputs.into_iter().zip(self.clear_inputs) {
            let secret = info.signature_secret;
            let signature = secret.sign(&unsigned_tx_data[..]);
            let input = ClearInput::from_partial(input, signature);
            clear_inputs.push(input);
        }

        let mut inputs = vec![];
        for (input, signature_secret) in
            partial.inputs.into_iter().zip(signature_secrets.into_iter())
        {
            let signature = signature_secret.sign(&unsigned_tx_data[..]);
            let input = Input::from_partial(input, signature);
            inputs.push(input);
        }

        let call_data = CallData { clear_inputs, inputs, outputs: partial.outputs };

        Ok(FuncCall {
            contract_id: "money".to_string(),
            func_id: "money::transfer()".to_string(),
            call_data: Box::new(call_data),
            proofs: partial.proofs,
        })
    }
}
