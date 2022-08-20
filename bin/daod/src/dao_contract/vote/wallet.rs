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
        keypair::{Keypair, PublicKey, SecretKey},
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
    dao_contract::vote::validate::{CallData, Header, Input},
    demo::{CallDataBase, FuncCall, StateRegistry, ZkContractInfo, ZkContractTable},
    money_contract, note,
    util::poseidon_hash,
};

#[derive(SerialEncodable, SerialDecodable)]
pub struct Note {
    vote: Vote,
    value: u64,
}

#[derive(SerialEncodable, SerialDecodable)]
// All info needed for vote and value commits
pub struct Vote {
    pub value_blind: pallas::Scalar,
    pub vote_option: bool,
    pub vote_option_blind: pallas::Scalar,
    //TODO: gov_token_id: pallas::Base,
}

pub struct BuilderInput {
    pub secret: SecretKey,
    pub note: money_contract::transfer::wallet::Note,
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
}

// TODO: Pass DAO and Proposal into Builder.
// Inside ZKproof, check proposal is correct.
pub struct Builder {
    pub inputs: Vec<BuilderInput>,
    pub vote: Vote,
    pub vote_keypair: Keypair,
}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        let mut proofs = vec![];

        let gov_token_blind = pallas::Base::random(&mut OsRng);
        let vote_blind = pallas::Scalar::random(&mut OsRng);

        let mut inputs = vec![];
        let mut total_value = 0;
        let mut total_value_blind = pallas::Scalar::from(0);
        let mut signature_secrets = vec![];

        for input in self.inputs {
            let value_blind = pallas::Scalar::random(&mut OsRng);

            total_value += input.note.value;
            total_value_blind += value_blind;

            let signature_secret = SecretKey::random(&mut OsRng);
            let signature_public = PublicKey::from_secret(signature_secret);

            let zk_info = zk_bins.lookup(&"dao-vote-burn".to_string()).unwrap();

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
                Witness::Base(Value::known(pallas::Base::from(0))),
                Witness::Base(Value::known(pallas::Base::from(0))),
                Witness::Base(Value::known(pallas::Base::from(note.value))),
                Witness::Base(Value::known(note.token_id)),
                Witness::Base(Value::known(note.coin_blind)),
                Witness::Scalar(Value::known(value_blind)),
                Witness::Base(Value::known(gov_token_blind)),
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

            let token_commit = poseidon_hash::<2>([note.token_id, gov_token_blind]);
            //TODO: assert_eq!(self.dao.gov_token_id, note.token_id);

            let value_commit = pedersen_commitment_u64(note.value, value_blind);
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
            let input_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
                .expect("DAO::vote() proving error!");
            proofs.push(input_proof);

            // First we make the tx then sign after
            signature_secrets.push(signature_secret);

            let input = Input { value_commit, merkle_root, signature_public };
            inputs.push(input);
        }

        //TODO: let token_commit = poseidon_hash::<2>([self.dao.gov_token_id, gov_token_blind]);

        let vote = self.vote.vote_option as u64;
        assert!(vote == 0 || vote == 1);

        let weighted_vote = vote * total_value;

        let vote_commit = pedersen_commitment_u64(weighted_vote, vote_blind);
        let vote_coords = vote_commit.to_affine().coordinates().unwrap();
        let vote_commit_x = *vote_coords.x();
        let vote_commit_y = *vote_coords.y();
        let vote = pallas::Base::from(vote);

        let total_value_commit = pedersen_commitment_u64(total_value, total_value_blind);
        let total_value_coords = total_value_commit.to_affine().coordinates().unwrap();
        let total_value_x = *total_value_coords.x();
        let total_value_y = *total_value_coords.y();
        let value_base = pallas::Base::from(total_value);

        let zk_info = zk_bins.lookup(&"dao-vote-main".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };
        let zk_bin = zk_info.bincode.clone();

        let prover_witnesses = vec![
            // Total number of gov tokens allocated
            Witness::Base(Value::known(value_base)),
            Witness::Scalar(Value::known(total_value_blind)),
            // Vote
            Witness::Base(Value::known(vote)),
            Witness::Scalar(Value::known(vote_blind)),
            // TODO: gov token
        ];

        let public_inputs = vec![
            //TODO: token_commit
            total_value_x,
            total_value_y,
            vote_commit_x,
            vote_commit_y,
        ];

        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

        let proving_key = &zk_info.proving_key;
        let main_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::vote() proving error!");
        proofs.push(main_proof);

        let note = Note { vote: self.vote, value: total_value };
        let enc_note = note::encrypt(&note, &self.vote_keypair.public).unwrap();

        let header = Header { enc_note };

        let mut unsigned_tx_data = vec![];
        header.encode(&mut unsigned_tx_data).expect("failed to encode data");
        inputs.encode(&mut unsigned_tx_data).expect("failed to encode inputs");
        proofs.encode(&mut unsigned_tx_data).expect("failed to encode proofs");

        let mut signatures = vec![];
        for signature_secret in &signature_secrets {
            let signature = signature_secret.sign(&unsigned_tx_data[..]);
            signatures.push(signature);
        }

        let call_data = CallData { header, inputs, signatures };

        FuncCall {
            contract_id: "DAO".to_string(),
            func_id: "DAO::vote()".to_string(),
            call_data: Box::new(call_data),
            proofs: vec![],
        }
    }
}
