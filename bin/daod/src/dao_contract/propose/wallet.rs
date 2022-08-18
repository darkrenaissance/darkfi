use halo2_proofs::circuit::Value;
use incrementalmerkletree::Hashable;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve, Group},
    pallas,
};
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        burn_proof::create_burn_proof,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        mint_proof::create_mint_proof,
        proof::ProvingKey,
        schnorr::SchnorrSecret,
        types::{
            DrkCircuitField, DrkCoinBlind, DrkSerial, DrkSpendHook, DrkTokenId, DrkUserData,
            DrkUserDataBlind, DrkValueBlind,
        },
        util::{pedersen_commitment_base, pedersen_commitment_u64},
        Proof,
    },
    util::serial::{Encodable, SerialDecodable, SerialEncodable},
    zk::vm::{Witness, ZkCircuit},
};

use crate::{
    dao_contract::propose::validate::{CallData, Header, Input},
    demo::{CallDataBase, FuncCall, StateRegistry, ZkContractInfo, ZkContractTable},
    money_contract,
    util::poseidon_hash,
};

pub struct BuilderInput {
    pub secret: SecretKey,
    pub note: money_contract::transfer::wallet::Note,
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
}

pub struct Proposal {
    pub dest: PublicKey,
    pub amount: u64,
    pub serial: pallas::Base,
    pub token_id: pallas::Base,
    pub blind: pallas::Base,
}

pub struct DaoParams {
    pub proposer_limit: u64,
    pub quorum: u64,
    pub approval_ratio: u64,
    pub gov_token_id: pallas::Base,
    pub public_key: PublicKey,
    pub bulla_blind: pallas::Base,
}

pub struct Builder {
    pub inputs: Vec<BuilderInput>,
    pub proposal: Proposal,
    pub dao: DaoParams,
    pub dao_leaf_position: incrementalmerkletree::Position,
    pub dao_merkle_path: Vec<MerkleNode>,
    pub dao_merkle_root: MerkleNode,
}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        let mut proofs = vec![];

        let token_blind = pallas::Base::random(&mut OsRng);

        let mut inputs = vec![];
        let mut total_funds = 0;
        let mut input_funds_blinds = vec![];
        let mut signature_secrets = vec![];
        for input in self.inputs {
            let funds_blind = pallas::Scalar::random(&mut OsRng);
            input_funds_blinds.push(funds_blind);

            let signature_secret = SecretKey::random(&mut OsRng);
            let signature_public = PublicKey::from_secret(signature_secret);

            let zk_info = zk_bins.lookup(&"dao-propose-burn".to_string()).unwrap();
            let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
                info
            } else {
                panic!("Not binary info")
            };
            let zk_bin = zk_info.bincode.clone();

            // Note from the previous output
            let note = input.note;
            let leaf_pos: u64 = input.leaf_position.into();

            let prover_witnesses = vec![
                Witness::Base(Value::known(input.secret.0)),
                Witness::Base(Value::known(note.serial)),
                Witness::Base(Value::known(pallas::Base::from(note.value))),
                Witness::Base(Value::known(note.token_id)),
                Witness::Base(Value::known(note.coin_blind)),
                Witness::Scalar(Value::known(funds_blind)),
                Witness::Base(Value::known(token_blind)),
                Witness::Uint32(Value::known(leaf_pos.try_into().unwrap())),
                Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
                Witness::Base(Value::known(signature_secret.0)),
            ];

            let public_key = PublicKey::from_secret(input.secret);
            let coords = public_key.0.to_affine().coordinates().unwrap();

            let coin = poseidon_hash::<8>([
                *coords.x(),
                *coords.y(),
                pallas::Base::from(note.value),
                note.token_id,
                note.serial,
                pallas::Base::from(0),
                pallas::Base::from(0),
                note.coin_blind,
            ]);

            let merkle_root = {
                let position: u64 = input.leaf_position.into();
                let mut current = MerkleNode(coin);
                for (level, sibling) in input.merkle_path.iter().enumerate() {
                    let level = level as u8;
                    current = if position & (1 << level) == 0 {
                        MerkleNode::combine(level.into(), &current, sibling)
                    } else {
                        MerkleNode::combine(level.into(), sibling, &current)
                    };
                }
                current
            };

            let token_commit = poseidon_hash::<2>([note.token_id, token_blind]);
            assert_eq!(self.dao.gov_token_id, note.token_id);

            let value_commit = pedersen_commitment_u64(note.value, funds_blind);
            let value_coords = value_commit.to_affine().coordinates().unwrap();
            let value_commit_x = *value_coords.x();
            let value_commit_y = *value_coords.y();

            let sigpub_coords = signature_public.0.to_affine().coordinates().unwrap();
            let sigpub_x = *sigpub_coords.x();
            let sigpub_y = *sigpub_coords.y();

            let public_inputs = vec![
                value_commit_x,
                value_commit_y,
                token_commit,
                merkle_root.0,
                sigpub_x,
                sigpub_y,
            ];
            let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

            let proving_key = &zk_info.proving_key;
            let main_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
                .expect("DAO::propose() proving error!");

            // First we make the tx then sign after
            signature_secrets.push(signature_secret);

            let input = Input { value_commit, merkle_root, signature_public };
            inputs.push(input);
        }

        let mut total_funds_blind = pallas::Scalar::from(0);
        for blind in &input_funds_blinds {
            total_funds_blind += blind;
        }
        let total_funds_commit = pedersen_commitment_u64(total_funds, total_funds_blind);
        let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
        let total_funds_x = *total_funds_coords.x();
        let total_funds_y = *total_funds_coords.y();
        let total_funds = pallas::Base::from(total_funds);

        let gov_token_blind = pallas::Base::random(&mut OsRng);
        let token_commit = poseidon_hash::<2>([self.dao.gov_token_id, gov_token_blind]);

        let proposal_dest_coords = self.proposal.dest.0.to_affine().coordinates().unwrap();
        let proposal_dest_x = *proposal_dest_coords.x();
        let proposal_dest_y = *proposal_dest_coords.y();

        let proposal_amount = pallas::Base::from(self.proposal.amount);

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio = pallas::Base::from(self.dao.approval_ratio);

        let dao_pubkey_coords = self.dao.public_key.0.to_affine().coordinates().unwrap();
        let dao_public_x = *dao_pubkey_coords.x();
        let dao_public_y = *dao_pubkey_coords.x();

        let dao_bulla = poseidon_hash::<8>([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio,
            self.dao.gov_token_id,
            dao_public_x,
            dao_public_y,
            self.dao.bulla_blind,
            // @tmp-workaround
            self.dao.bulla_blind,
        ]);

        let dao_leaf_position: u64 = self.dao_leaf_position.into();

        let proposal_bulla = poseidon_hash::<8>([
            proposal_dest_x,
            proposal_dest_y,
            proposal_amount,
            self.proposal.serial,
            self.proposal.token_id,
            dao_bulla,
            self.proposal.blind,
            // @tmp-workaround
            self.proposal.blind,
        ]);

        let zk_info = zk_bins.lookup(&"dao-propose-main".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };
        let zk_bin = zk_info.bincode.clone();
        let prover_witnesses = vec![
            // Proposers total number of gov tokens
            Witness::Base(Value::known(total_funds)),
            Witness::Scalar(Value::known(total_funds_blind)),
            // Used for blinding exported gov token ID
            Witness::Base(Value::known(gov_token_blind)),
            // proposal params
            Witness::Base(Value::known(proposal_dest_x)),
            Witness::Base(Value::known(proposal_dest_y)),
            Witness::Base(Value::known(proposal_amount)),
            Witness::Base(Value::known(self.proposal.serial)),
            Witness::Base(Value::known(self.proposal.token_id)),
            Witness::Base(Value::known(self.proposal.blind)),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio)),
            Witness::Base(Value::known(self.dao.gov_token_id)),
            Witness::Base(Value::known(dao_public_x)),
            Witness::Base(Value::known(dao_public_y)),
            Witness::Base(Value::known(self.dao.bulla_blind)),
            Witness::Uint32(Value::known(dao_leaf_position.try_into().unwrap())),
            Witness::MerklePath(Value::known(self.dao_merkle_path.try_into().unwrap())),
        ];
        let public_inputs = vec![
            token_commit,
            self.dao_merkle_root.0,
            proposal_bulla,
            total_funds_x,
            total_funds_y,
        ];
        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

        let proving_key = &zk_info.proving_key;
        let main_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::propose() proving error!");
        proofs.push(main_proof);

        let header = Header {
            dao_merkle_root: self.dao_merkle_root,
            proposal_bulla,
            token_commit,
            total_funds_commit,
        };

        let mut unsigned_tx_data = vec![];
        header.encode(&mut unsigned_tx_data).expect("failed to encode data");
        inputs.encode(&mut unsigned_tx_data).expect("failed to encode inputs");
        proofs.encode(&mut unsigned_tx_data).expect("failed to encode proofs");

        let mut signatures = vec![];
        for signature_secret in &signature_secrets {
            let signature = signature_secret.sign(&unsigned_tx_data[..]);
            signatures.push(signature);
        }

        let call_data = CallData { header, inputs: vec![], signatures: vec![] };

        FuncCall {
            contract_id: "DAO".to_string(),
            func_id: "DAO::propose()".to_string(),
            call_data: Box::new(call_data),
            proofs,
        }
    }
}
