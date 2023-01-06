/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::io::Cursor;

use darkfi_sdk::{
    crypto::{
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        ContractId, MerkleNode, MerkleTree, PublicKey,
    },
    db::{
        db_contains_key, db_del, db_get, db_init, db_lookup, db_set, SMART_CONTRACT_ZKAS_DB_NAME,
    },
    error::{ContractError, ContractResult},
    merkle::merkle_add,
    msg,
    pasta::{
        arithmetic::CurveAffine,
        group::{Curve, Group},
        pallas,
    },
    tx::ContractCall,
    util::set_return_data,
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable, WriteExt};

use darkfi_money_contract::{
    state::MoneyTransferParams, MoneyFunction, MONEY_CONTRACT_COIN_ROOTS_TREE,
    MONEY_CONTRACT_NULLIFIERS_TREE,
};

use crate::{
    dao_model::{
        DaoExecParams, DaoExecUpdate, DaoMintParams, DaoMintUpdate, DaoProposeParams,
        DaoProposeUpdate, DaoVoteParams, DaoVoteUpdate, ProposalVotes,
    },
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_EXEC_NS, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// These are the different sled trees that will be created
pub const DAO_BULLA_TREE: &str = "dao_info";
pub const DAO_ROOTS_TREE: &str = "dao_roots";
//pub const DAO_PROPOSAL_TREE: &str = "dao_proposals";
//pub const DAO_PROPOSAL_ROOTS_TREE: &str = "dao_proposal_roots";
pub const DAO_PROPOSAL_VOTES_TREE: &str = "dao_proposal_votes";
pub const DAO_VOTE_NULLS: &str = "dao_vote_nulls";

// These are keys inside the some db trees
pub const DAO_MERKLE_TREE: &str = "dao_merkle_tree";
pub const DAO_PROPOSAL_MERKLE_TREE: &str = "dao_proposals_merkle_tree";

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // The zkas circuits can simply be embedded in the wasm and set up by
    // the initialization. Note that the tree should then be called "zkas".
    // The lookups can then be done by `contract_id+_zkas+namespace`.
    let zkas_db = match db_lookup(cid, SMART_CONTRACT_ZKAS_DB_NAME) {
        Ok(v) => v,
        Err(_) => db_init(cid, SMART_CONTRACT_ZKAS_DB_NAME)?,
    };
    let dao_exec_bin = include_bytes!("../proof/dao-exec.zk.bin");
    let dao_mint_bin = include_bytes!("../proof/dao-mint.zk.bin");
    let dao_vote_burn_bin = include_bytes!("../proof/dao-vote-burn.zk.bin");
    let dao_vote_main_bin = include_bytes!("../proof/dao-vote-main.zk.bin");
    let dao_propose_burn_bin = include_bytes!("../proof/dao-propose-burn.zk.bin");
    let dao_propose_main_bin = include_bytes!("../proof/dao-propose-main.zk.bin");

    db_set(zkas_db, &serialize(&DAO_CONTRACT_ZKAS_DAO_EXEC_NS), &dao_exec_bin[..])?;
    db_set(zkas_db, &serialize(&DAO_CONTRACT_ZKAS_DAO_MINT_NS), &dao_mint_bin[..])?;
    db_set(zkas_db, &serialize(&DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS), &dao_vote_burn_bin[..])?;
    db_set(zkas_db, &serialize(&DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS), &dao_vote_main_bin[..])?;
    db_set(zkas_db, &serialize(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS), &dao_propose_burn_bin[..])?;
    db_set(zkas_db, &serialize(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS), &dao_propose_main_bin[..])?;

    // Set up a database tree to hold the Merkle tree for DAO bullas
    let dao_bulla_db = match db_lookup(cid, DAO_BULLA_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_BULLA_TREE)?,
    };

    match db_get(dao_bulla_db, &serialize(&DAO_MERKLE_TREE))? {
        Some(bytes) => {
            // We found some bytes, try to deserialize into a tree.
            // For now, if this doesn't work, we bail.
            let mut decoder = Cursor::new(&bytes);
            <i32 as Decodable>::decode(&mut decoder)?;
            <MerkleTree as Decodable>::decode(&mut decoder)?;
        }
        None => {
            // We didn't find a tree, so just make a new one.
            let tree = MerkleTree::new(100);
            let mut tree_data = vec![];

            tree_data.write_u32(0)?;
            tree.encode(&mut tree_data)?;
            db_set(dao_bulla_db, &serialize(&DAO_MERKLE_TREE), &tree_data)?;
        }
    };

    // Set up a database tree to hold Merkle roots for the DAO bullas Merkle tree
    let _ = match db_lookup(cid, DAO_ROOTS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_ROOTS_TREE)?,
    };

    // Set up a database tree to hold the Merkle tree for proposal bullas
    /*
    let dao_proposal_db = match db_lookup(cid, DAO_PROPOSAL_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_PROPOSAL_TREE)?,
    };
    */

    /*
    match db_get(dao_proposal_db, &serialize(&DAO_PROPOSAL_MERKLE_TREE))? {
        Some(bytes) => {
            // We found some bytes, try to deserialize into a tree.
            // For now, if this doesn't work, we bail.
            let _: MerkleTree = deserialize(&bytes)?;
        }
        None => {
            // We didn't find a tree, so just make a new one.
            let tree = MerkleTree::new(100);
            let mut tree_data = vec![];

            tree_data.write_u32(0)?;
            tree.encode(&mut tree_data)?;
            db_set(dao_proposal_db, &serialize(&DAO_PROPOSAL_MERKLE_TREE), &tree_data)?;
        }
    };
    */

    // Set up a database tree to hold Merkle roots for the proposal bullas Merkle tree
    /*let _ = match db_lookup(cid, DAO_PROPOSAL_ROOTS_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_PROPOSAL_ROOTS_TREE)?,
    };*/

    // Set up a database tree to hold proposal votes (k: proposalbulla, v: ProposalVotes)
    let _ = match db_lookup(cid, DAO_PROPOSAL_VOTES_TREE) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_PROPOSAL_VOTES_TREE)?,
    };

    let _ = match db_lookup(cid, DAO_VOTE_NULLS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DAO_VOTE_NULLS)?,
    };

    Ok(())
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    match DaoFunction::try_from(self_.data[0])? {
        DaoFunction::Mint => {
            let params: DaoMintParams = deserialize(&self_.data[1..])?;

            // No checks in Mint, just return the update.
            // TODO: Should it check that there isn't an existing one?
            let update = DaoMintUpdate { dao_bulla: params.dao_bulla };
            let mut update_data = vec![];
            update_data.write_u8(DaoFunction::Mint as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("[DAO Mint] State update set!");

            Ok(())
        }

        DaoFunction::Propose => {
            let params: DaoProposeParams = deserialize(&self_.data[1..])?;

            // Check the Merkle roots for the input coins are valid
            let money_cid = *MONEY_CONTRACT_ID;
            let coin_roots_db = db_lookup(money_cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
            for input in &params.inputs {
                if !db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
                    msg!("Invalid input Merkle root: {}", input.merkle_root);
                    return Err(ContractError::Custom(2))
                }
            }

            // Is the DAO bulla generated in the ZK proof valid
            let dao_roots_db = db_lookup(cid, DAO_ROOTS_TREE)?;
            if !db_contains_key(dao_roots_db, &serialize(&params.dao_merkle_root))? {
                msg!("Invalid DAO Merkle root: {}", params.dao_merkle_root);
                return Err(ContractError::Custom(3))
            }

            // TODO: Look at gov tokens avoid using already spent ones
            // Need to spend original coin and generate 2 nullifiers?

            let update = DaoProposeUpdate { proposal_bulla: params.proposal_bulla };
            let mut update_data = vec![];
            update_data.write_u8(DaoFunction::Propose as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("[DAO Propose] State update set!");

            Ok(())
        }

        DaoFunction::Vote => {
            let params: DaoVoteParams = deserialize(&self_.data[1..])?;

            let money_cid = *MONEY_CONTRACT_ID;

            // Check proposal bulla exists
            let proposal_votes_db = db_lookup(cid, DAO_PROPOSAL_VOTES_TREE)?;
            let Some(proposal_votes) = db_get(proposal_votes_db, &serialize(&params.proposal_bulla))? else {
                msg!("Invalid proposal {:?}", params.proposal_bulla);
                return Err(ContractError::Custom(4))
            };
            let mut proposal_votes: ProposalVotes = deserialize(&proposal_votes)?;

            // Check the Merkle roots and nullifiers for the input coins are valid
            // TODO: vote_nullifiers is useless
            let money_roots_db = db_lookup(money_cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
            let money_nullifier_db = db_lookup(money_cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
            let dao_vote_nulls_db = db_lookup(cid, DAO_VOTE_NULLS)?;

            for input in &params.inputs {
                if !db_contains_key(money_roots_db, &serialize(&input.merkle_root))? {
                    msg!("Invalid input Merkle root: {:?}", input.merkle_root);
                    return Err(ContractError::Custom(5))
                }

                if db_contains_key(money_nullifier_db, &serialize(&input.nullifier))? {
                    msg!("Coin is already spent");
                    return Err(ContractError::Custom(6))
                }

                if proposal_votes.vote_nullifiers.contains(&input.nullifier) ||
                    db_contains_key(dao_vote_nulls_db, &serialize(&input.nullifier))?
                {
                    msg!("Attempted double vote");
                    return Err(ContractError::Custom(7))
                }

                proposal_votes.all_votes_commit += input.vote_commit;
                proposal_votes.vote_nullifiers.push(input.nullifier);
            }

            proposal_votes.yes_votes_commit += params.yes_vote_commit;

            let update = DaoVoteUpdate {
                proposal_bulla: params.proposal_bulla,
                proposal_votes, //vote_nullifiers,
                                //yes_vote_commit: params.yes_vote_commit,
                                //all_vote_commit,
            };
            let mut update_data = vec![];
            update_data.write_u8(DaoFunction::Vote as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("[DAO Vote] State update set!");

            Ok(())
        }

        DaoFunction::Exec => {
            let params: DaoExecParams = deserialize(&self_.data[1..])?;

            // =============================
            // Enforce tx has correct format
            // =============================
            // 1. There should be only two calls
            assert!(call.len() == 2);

            // 2. func_call_index == 1
            assert!(call_idx == 1);

            // 3. First item should be a MoneyTransfer call
            assert!(call[0].contract_id == *MONEY_CONTRACT_ID);
            assert!(call[0].data[0] == MoneyFunction::Transfer as u8);

            // 4. MoneyTransfer has exactly 2 outputs
            let mt_params: MoneyTransferParams = deserialize(&call[0].data[1..])?;
            assert!(mt_params.outputs.len() == 2);

            // ======
            // Checks
            // ======
            // 1. Check both coins in MoneyTransfer are equal to our coin_0, coin_1
            assert!(mt_params.outputs[0].coin == params.coin_0);
            assert!(mt_params.outputs[1].coin == params.coin_1);

            // 2. Sum of MoneyTransfer input value commits == our input value commit
            let mut input_valcoms = pallas::Point::identity();
            for input in mt_params.inputs {
                input_valcoms += input.value_commit;
            }
            assert!(input_valcoms == params.input_value_commit);

            // 3. Get the ProposalVote from DAO state
            let proposal_db = db_lookup(cid, DAO_PROPOSAL_VOTES_TREE)?;
            let Some(proposal_votes) = db_get(proposal_db, &serialize(&params.proposal))? else {
                msg!("Proposal {:?} not found in db", params.proposal);
                return Err(ContractError::Custom(1));
            };
            let proposal_votes: ProposalVotes = deserialize(&proposal_votes)?;

            // 4. Check yes_votes_commit and all_votes_commit are the same as in ProposalVotes
            assert!(proposal_votes.yes_votes_commit == params.yes_votes_commit);
            assert!(proposal_votes.all_votes_commit == params.all_votes_commit);

            let update = DaoExecUpdate { proposal: params.proposal };
            let mut update_data = vec![];
            update_data.write_u8(DaoFunction::Exec as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("[DAO Exec] State update set!");

            Ok(())
        }
    }
}

fn process_update(cid: ContractId, ix: &[u8]) -> ContractResult {
    match DaoFunction::try_from(ix[0])? {
        DaoFunction::Mint => {
            let update: DaoMintUpdate = deserialize(&ix[1..])?;

            let bulla_db = db_lookup(cid, DAO_BULLA_TREE)?;
            let roots_db = db_lookup(cid, DAO_ROOTS_TREE)?;

            let node = MerkleNode::from(update.dao_bulla.inner());
            merkle_add(bulla_db, roots_db, &serialize(&DAO_MERKLE_TREE), &[node])?;

            Ok(())
        }

        DaoFunction::Propose => {
            let update: DaoProposeUpdate = deserialize(&ix[1..])?;

            //let proposal_tree_db = db_lookup(cid, DAO_PROPOSAL_TREE)?;
            //let proposal_root_db = db_lookup(cid, DAO_PROPOSAL_ROOTS_TREE)?;
            let proposal_vote_db = db_lookup(cid, DAO_PROPOSAL_VOTES_TREE)?;

            /*
            let node = MerkleNode::from(update.proposal_bulla);
            merkle_add(
                proposal_tree_db,
                proposal_root_db,
                &serialize(&DAO_PROPOSAL_MERKLE_TREE),
                &[node],
            )?;
            */

            let pv = ProposalVotes::default();
            db_set(proposal_vote_db, &serialize(&update.proposal_bulla), &serialize(&pv))?;

            Ok(())
        }

        DaoFunction::Vote => {
            let update: DaoVoteUpdate = deserialize(&ix[1..])?;

            // Perform this code:
            //votes_info.yes_votes_commit += self.yes_vote_commit;
            //votes_info.all_votes_commit += self.all_vote_commit;
            //votes_info.vote_nulls.append(&mut self.vote_nulls);

            let proposal_vote_db = db_lookup(cid, DAO_PROPOSAL_VOTES_TREE)?;
            db_set(
                proposal_vote_db,
                &serialize(&update.proposal_bulla),
                &serialize(&update.proposal_votes),
            )?;

            let dao_vote_nulls_db = db_lookup(cid, DAO_VOTE_NULLS)?;

            for nullifier in update.proposal_votes.vote_nullifiers {
                db_set(dao_vote_nulls_db, &serialize(&nullifier), &[])?;
            }

            Ok(())
        }

        DaoFunction::Exec => {
            let update: DaoExecUpdate = deserialize(&ix[1..])?;

            // Remove proposal from db
            let proposal_vote_db = db_lookup(cid, DAO_PROPOSAL_VOTES_TREE)?;
            db_del(proposal_vote_db, &serialize(&update.proposal))?;

            Ok(())
        }
    }
}

fn get_metadata(_: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    match DaoFunction::try_from(self_.data[0])? {
        DaoFunction::Mint => {
            let params: DaoMintParams = deserialize(&self_.data[1..])?;

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
            // TODO: Why no signatures? Should it be signed with the DAO keypair?
            let signature_pubkeys: Vec<PublicKey> = vec![];

            zk_public_values
                .push((DAO_CONTRACT_ZKAS_DAO_MINT_NS.to_string(), vec![params.dao_bulla.inner()]));

            let mut metadata = vec![];
            zk_public_values.encode(&mut metadata)?;
            signature_pubkeys.encode(&mut metadata)?;

            // Using this, we pass the above data to the host.
            set_return_data(&metadata)?;
            Ok(())
        }

        DaoFunction::Propose => {
            let params: DaoProposeParams = deserialize(&self_.data[1..])?;
            assert!(!params.inputs.is_empty());

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
            let mut signature_pubkeys: Vec<PublicKey> = vec![];

            let mut total_funds_commit = pallas::Point::identity();

            for input in &params.inputs {
                signature_pubkeys.push(input.signature_public);
                total_funds_commit += input.value_commit;

                let value_coords = input.value_commit.to_affine().coordinates().unwrap();
                let (sig_x, sig_y) = input.signature_public.xy();

                zk_public_values.push((
                    DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS.to_string(),
                    vec![
                        *value_coords.x(),
                        *value_coords.y(),
                        params.token_commit,
                        input.merkle_root.inner(),
                        sig_x,
                        sig_y,
                    ],
                ));
            }

            let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
            zk_public_values.push((
                DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS.to_string(),
                vec![
                    params.token_commit,
                    params.dao_merkle_root.inner(),
                    params.proposal_bulla,
                    *total_funds_coords.x(),
                    *total_funds_coords.y(),
                ],
            ));

            let mut metadata = vec![];
            zk_public_values.encode(&mut metadata)?;
            signature_pubkeys.encode(&mut metadata)?;

            // Using this, we pass the above data to the host.
            set_return_data(&metadata)?;
            Ok(())
        }

        DaoFunction::Vote => {
            let params: DaoVoteParams = deserialize(&self_.data[1..])?;
            assert!(!params.inputs.is_empty());

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
            let mut signature_pubkeys: Vec<PublicKey> = vec![];

            let mut all_votes_commit = pallas::Point::identity();

            for input in &params.inputs {
                signature_pubkeys.push(input.signature_public);
                all_votes_commit += input.vote_commit;

                let value_coords = input.vote_commit.to_affine().coordinates().unwrap();
                let (sig_x, sig_y) = input.signature_public.xy();

                zk_public_values.push((
                    DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS.to_string(),
                    vec![
                        input.nullifier.inner(),
                        *value_coords.x(),
                        *value_coords.y(),
                        params.token_commit,
                        input.merkle_root.inner(),
                        sig_x,
                        sig_y,
                    ],
                ));
            }

            let yes_vote_commit_coords = params.yes_vote_commit.to_affine().coordinates().unwrap();
            let all_vote_commit_coords = all_votes_commit.to_affine().coordinates().unwrap();

            zk_public_values.push((
                DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS.to_string(),
                vec![
                    params.token_commit,
                    params.proposal_bulla,
                    *yes_vote_commit_coords.x(),
                    *yes_vote_commit_coords.y(),
                    *all_vote_commit_coords.x(),
                    *all_vote_commit_coords.y(),
                ],
            ));

            let mut metadata = vec![];
            zk_public_values.encode(&mut metadata)?;
            signature_pubkeys.encode(&mut metadata)?;

            // Using this, we pass the above data to the host.
            set_return_data(&metadata)?;
            Ok(())
        }

        DaoFunction::Exec => {
            let params: DaoExecParams = deserialize(&self_.data[1..])?;

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
            let signature_pubkeys: Vec<PublicKey> = vec![];

            let yes_votes_coords = params.yes_votes_commit.to_affine().coordinates().unwrap();
            let all_votes_coords = params.all_votes_commit.to_affine().coordinates().unwrap();
            let input_value_coords = params.input_value_commit.to_affine().coordinates().unwrap();

            msg!("params.proposal: {:?}", params.proposal);
            zk_public_values.push((
                DAO_CONTRACT_ZKAS_DAO_EXEC_NS.to_string(),
                vec![
                    params.proposal,
                    params.coin_0,
                    params.coin_1,
                    *yes_votes_coords.x(),
                    *yes_votes_coords.y(),
                    *all_votes_coords.x(),
                    *all_votes_coords.y(),
                    *input_value_coords.x(),
                    *input_value_coords.y(),
                    DAO_CONTRACT_ID.inner(),
                    pallas::Base::zero(),
                    pallas::Base::zero(),
                ],
            ));

            let mut metadata = vec![];
            zk_public_values.encode(&mut metadata)?;
            signature_pubkeys.encode(&mut metadata)?;

            // Using this, we pass the above data to the host.
            set_return_data(&metadata)?;
            Ok(())
        }
    }
}
