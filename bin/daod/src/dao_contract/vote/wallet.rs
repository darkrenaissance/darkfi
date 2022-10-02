use darkfi::{
    crypto::{
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        nullifier::Nullifier,
        util::{pedersen_commitment_u64, poseidon_hash},
        Proof,
    },
    serial::{SerialDecodable, SerialEncodable},
    zk::vm::{Witness, ZkCircuit},
};
use halo2_proofs::circuit::Value;
use incrementalmerkletree::Hashable;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use rand::rngs::OsRng;

use crate::{
    dao_contract::{
        mint::wallet::DaoParams,
        propose::wallet::Proposal,
        vote::validate::{CallData, Header, Input},
        CONTRACT_ID,
    },
    demo::{FuncCall, ZkContractInfo, ZkContractTable},
    money_contract, note,
};

use log::debug;

#[derive(SerialEncodable, SerialDecodable)]
pub struct Note {
    pub vote: Vote,
    pub vote_value: u64,
    pub vote_value_blind: pallas::Scalar,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct Vote {
    pub vote_option: bool,
    pub vote_option_blind: pallas::Scalar,
}

pub struct BuilderInput {
    pub secret: SecretKey,
    pub note: money_contract::transfer::wallet::Note,
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub signature_secret: SecretKey,
}

// TODO: should be token locking voting?
// Inside ZKproof, check proposal is correct.
pub struct Builder {
    pub inputs: Vec<BuilderInput>,
    pub vote: Vote,
    pub vote_keypair: Keypair,
    pub proposal: Proposal,
    pub dao: DaoParams,
}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        debug!(target: "dao_contract::vote::wallet::Builder", "build()");
        let mut proofs = vec![];

        let gov_token_blind = pallas::Base::random(&mut OsRng);

        let mut inputs = vec![];
        let mut vote_value = 0;
        let mut vote_value_blind = pallas::Scalar::from(0);

        for input in self.inputs {
            let value_blind = pallas::Scalar::random(&mut OsRng);

            vote_value += input.note.value;
            vote_value_blind += value_blind;

            let signature_public = PublicKey::from_secret(input.signature_secret);

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
                Witness::Scalar(Value::known(vote_value_blind)),
                Witness::Base(Value::known(gov_token_blind)),
                Witness::Uint32(Value::known(leaf_pos.try_into().unwrap())),
                Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
                Witness::Base(Value::known(input.signature_secret.0)),
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
            assert_eq!(self.dao.gov_token_id, note.token_id);

            let nullifier = poseidon_hash::<2>([input.secret.0, note.serial]);

            let vote_commit = pedersen_commitment_u64(note.value, vote_value_blind);
            let vote_commit_coords = vote_commit.to_affine().coordinates().unwrap();

            let sigpub_coords = signature_public.0.to_affine().coordinates().unwrap();

            let public_inputs = vec![
                nullifier,
                *vote_commit_coords.x(),
                *vote_commit_coords.y(),
                token_commit,
                merkle_root.0,
                *sigpub_coords.x(),
                *sigpub_coords.y(),
            ];

            let circuit = ZkCircuit::new(prover_witnesses, zk_bin);
            let proving_key = &zk_info.proving_key;
            debug!(target: "dao_contract::vote::wallet::Builder", "input_proof Proof::create()");
            let input_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
                .expect("DAO::vote() proving error!");
            proofs.push(input_proof);

            let input = Input {
                nullifier: Nullifier(nullifier),
                vote_commit,
                merkle_root,
                signature_public,
            };
            inputs.push(input);
        }

        let token_commit = poseidon_hash::<2>([self.dao.gov_token_id, gov_token_blind]);

        let proposal_dest_coords = self.proposal.dest.0.to_affine().coordinates().unwrap();

        let proposal_amount = pallas::Base::from(self.proposal.amount);

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);

        let dao_pubkey_coords = self.dao.public_key.0.to_affine().coordinates().unwrap();

        let dao_bulla = poseidon_hash::<8>([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            self.dao.gov_token_id,
            *dao_pubkey_coords.x(),
            *dao_pubkey_coords.y(),
            self.dao.bulla_blind,
        ]);

        let proposal_bulla = poseidon_hash::<8>([
            *proposal_dest_coords.x(),
            *proposal_dest_coords.y(),
            proposal_amount,
            self.proposal.serial,
            self.proposal.token_id,
            dao_bulla,
            self.proposal.blind,
            // @tmp-workaround
            self.proposal.blind,
        ]);

        let vote_option = self.vote.vote_option as u64;
        assert!(vote_option == 0 || vote_option == 1);

        let yes_vote_commit =
            pedersen_commitment_u64(vote_option * vote_value, self.vote.vote_option_blind);
        let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();

        let all_vote_commit = pedersen_commitment_u64(vote_value, vote_value_blind);
        let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

        let zk_info = zk_bins.lookup(&"dao-vote-main".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };
        let zk_bin = zk_info.bincode.clone();

        let prover_witnesses = vec![
            // proposal params
            Witness::Base(Value::known(*proposal_dest_coords.x())),
            Witness::Base(Value::known(*proposal_dest_coords.y())),
            Witness::Base(Value::known(proposal_amount)),
            Witness::Base(Value::known(self.proposal.serial)),
            Witness::Base(Value::known(self.proposal.token_id)),
            Witness::Base(Value::known(self.proposal.blind)),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.dao.gov_token_id)),
            Witness::Base(Value::known(*dao_pubkey_coords.x())),
            Witness::Base(Value::known(*dao_pubkey_coords.y())),
            Witness::Base(Value::known(self.dao.bulla_blind)),
            // Vote
            Witness::Base(Value::known(pallas::Base::from(vote_option))),
            Witness::Scalar(Value::known(self.vote.vote_option_blind)),
            // Total number of gov tokens allocated
            Witness::Base(Value::known(pallas::Base::from(vote_value))),
            Witness::Scalar(Value::known(vote_value_blind)),
            // gov token
            Witness::Base(Value::known(gov_token_blind)),
        ];

        let public_inputs = vec![
            token_commit,
            proposal_bulla,
            // this should be a value commit??
            *yes_vote_commit_coords.x(),
            *yes_vote_commit_coords.y(),
            *all_vote_commit_coords.x(),
            *all_vote_commit_coords.y(),
        ];

        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

        let proving_key = &zk_info.proving_key;
        debug!(target: "dao_contract::vote::wallet::Builder", "main_proof = Proof::create()");
        let main_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::vote() proving error!");
        proofs.push(main_proof);

        let note = Note { vote: self.vote, vote_value, vote_value_blind };
        let enc_note = note::encrypt(&note, &self.vote_keypair.public).unwrap();

        let header = Header { token_commit, proposal_bulla, yes_vote_commit, enc_note };

        let call_data = CallData { header, inputs };

        FuncCall {
            contract_id: *CONTRACT_ID,
            func_id: *super::FUNC_ID,
            call_data: Box::new(call_data),
            proofs,
        }
    }
}
