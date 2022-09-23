use pasta_curves::group::ff::Field;
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        burn_proof::create_burn_proof,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        mint_proof::create_mint_proof,
        types::{
            DrkCoinBlind, DrkSerial, DrkSpendHook, DrkTokenId, DrkUserData, DrkUserDataBlind,
            DrkValueBlind,
        },
    },
    util::serial::{SerialDecodable, SerialEncodable},
    Result,
};

use crate::{
    demo::{FuncCall, ZkContractInfo, ZkContractTable},
    money_contract::{
        transfer::validate::{CallData, ClearInput, Input, Output},
        CONTRACT_ID,
    },
    note,
};

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Note {
    pub serial: DrkSerial,
    pub value: u64,
    pub token_id: DrkTokenId,
    pub spend_hook: DrkSpendHook,
    pub user_data: DrkUserData,
    pub coin_blind: DrkCoinBlind,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
}

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
    pub user_data_blind: DrkUserDataBlind,
    pub value_blind: DrkValueBlind,
    pub signature_secret: SecretKey,
}

pub struct BuilderOutputInfo {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub public: PublicKey,
    pub serial: DrkSerial,
    pub coin_blind: DrkCoinBlind,
    pub spend_hook: DrkSpendHook,
    pub user_data: DrkUserData,
}

impl Builder {
    fn compute_remainder_blind(
        clear_inputs: &[ClearInput],
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

    pub fn build(self, zk_bins: &ZkContractTable) -> Result<FuncCall> {
        assert!(self.clear_inputs.len() + self.inputs.len() > 0);

        let mut clear_inputs = vec![];
        let token_blind = DrkValueBlind::random(&mut OsRng);
        for input in &self.clear_inputs {
            let signature_public = PublicKey::from_secret(input.signature_secret);
            let value_blind = DrkValueBlind::random(&mut OsRng);

            let clear_input = ClearInput {
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

        for input in self.inputs {
            let value_blind = input.value_blind;
            input_blinds.push(value_blind);

            let zk_info = zk_bins.lookup(&"money-transfer-burn".to_string()).unwrap();
            let zk_info = if let ZkContractInfo::Native(info) = zk_info {
                info
            } else {
                panic!("Not native info")
            };
            let burn_pk = &zk_info.proving_key;

            // Note from the previous output
            let note = input.note.clone();

            let (burn_proof, revealed) = create_burn_proof(
                burn_pk,
                note.value,
                note.token_id,
                value_blind,
                token_blind,
                note.serial,
                note.spend_hook,
                note.user_data,
                input.user_data_blind,
                note.coin_blind,
                input.secret,
                input.leaf_position,
                input.merkle_path.clone(),
                input.signature_secret,
            )?;
            proofs.push(burn_proof);

            let input = Input { revealed };
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

            let serial = output.serial;
            let coin_blind = output.coin_blind;

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
                output.spend_hook,
                output.user_data,
                coin_blind,
                output.public,
            )?;
            proofs.push(mint_proof);

            let note = Note {
                serial,
                value: output.value,
                token_id: output.token_id,
                spend_hook: output.spend_hook,
                user_data: output.user_data,
                coin_blind,
                value_blind,
                token_blind,
            };

            let encrypted_note = note::encrypt(&note, &output.public)?;

            let output = Output { revealed, enc_note: encrypted_note };
            outputs.push(output);
        }

        let call_data = CallData { clear_inputs, inputs, outputs };

        Ok(FuncCall {
            contract_id: *CONTRACT_ID,
            func_id: *super::FUNC_ID,
            call_data: Box::new(call_data),
            proofs,
        })
    }
}
