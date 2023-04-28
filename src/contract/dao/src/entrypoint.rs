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
        pallas, pasta_prelude::*, ContractId, MerkleNode, MerkleTree, PublicKey, DAO_CONTRACT_ID,
        MONEY_CONTRACT_ID,
    },
    db::{db_contains_key, db_del, db_get, db_init, db_lookup, db_set, zkas_db_set},
    error::{ContractError, ContractResult},
    merkle_add, msg,
    util::set_return_data,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable, WriteExt};

use darkfi_money_contract::{
    model::MoneyTransferParamsV1 as MoneyTransferParams,
    MoneyFunction::TransferV1 as MoneyTransfer, MONEY_CONTRACT_COIN_ROOTS_TREE,
    MONEY_CONTRACT_NULLIFIERS_TREE,
};

use crate::{
    dao_model::{
        DaoBlindAggregateVote, DaoExecParams, DaoExecUpdate, DaoMintParams, DaoMintUpdate,
        DaoProposeParams, DaoProposeUpdate, DaoVoteParams, DaoVoteUpdate,
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

/// General info for the DAO
pub const DB_INFO: &str = "dao_info";
/// Name of the DAO bulla tree in DB_INFO
pub const KEY_DAO_MERKLE_TREE: &str = "dao_merkle_tree";

/// DAO bullas
pub const DB_DAO_BULLAS: &str = "dao_bullas";
/// Keeps track of all merkle roots DAO bullas
pub const DB_DAO_MERKLE_ROOTS: &str = "dao_roots";

/// Proposal bullas. The key is the current aggregated vote
pub const DB_PROPOSAL_BULLAS: &str = "dao_proposals";
/// Nullifiers to prevent double voting
pub const DAO_VOTE_NULLS: &str = "dao_vote_nulls";

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // The zkas circuits can simply be embedded in the wasm and set up by
    // the initialization.
    zkas_db_set(&include_bytes!("../proof/dao-exec.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-mint.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-vote-burn.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-vote-main.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-propose-burn.zk.bin")[..])?;
    zkas_db_set(&include_bytes!("../proof/dao-propose-main.zk.bin")[..])?;

    // Setup db for general info
    let dao_info_db = match db_lookup(cid, DB_INFO) {
        Ok(v) => v,
        Err(_) => db_init(cid, DB_INFO)?,
    };

    // Setup the entries in the header table
    match db_get(dao_info_db, &serialize(&KEY_DAO_MERKLE_TREE))? {
        Some(bytes) => {
            // We found some bytes, try to deserialize into a tree.
            // For now, if this doesn't work, we bail.
            let mut decoder = Cursor::new(&bytes);
            <u32 as Decodable>::decode(&mut decoder)?;
            <Vec<MerkleTree> as Decodable>::decode(&mut decoder)?;
        }
        None => {
            // We didn't find a tree, so just make a new one.
            let tree = MerkleTree::new(100);

            let mut tree_data = vec![];
            tree_data.write_u32(0)?;
            tree.encode(&mut tree_data)?;

            db_set(dao_info_db, &serialize(&KEY_DAO_MERKLE_TREE), &tree_data)?;
        }
    };

    // Setup db to avoid double creating DAOs
    let _ = match db_lookup(cid, DB_DAO_BULLAS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DB_DAO_BULLAS)?,
    };

    // Setup db for DAO bulla merkle roots
    let _ = match db_lookup(cid, DB_DAO_MERKLE_ROOTS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DB_DAO_MERKLE_ROOTS)?,
    };

    // Setup db for proposal votes (k: ProposalBulla, v: BlindAggregateVote)
    let _ = match db_lookup(cid, DB_PROPOSAL_BULLAS) {
        Ok(v) => v,
        Err(_) => db_init(cid, DB_PROPOSAL_BULLAS)?,
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
    let func = DaoFunction::try_from(self_.data[0])?;

    if call.len() != 1 {
        // Enforce a strict structure for our tx
        assert_eq!(call.len(), 2);
        assert_eq!(call_idx, 1);

        // We can unpack user_data and check the function call is correct.
        // But in this contract, only DAO::exec() can be invoked by other ones.
        // So just check the function call is correct.

        // NOTE: we may wish to improve this since it cripples user composability.

        assert_eq!(func, DaoFunction::Exec);
    }

    match func {
        DaoFunction::Mint => {
            let params: DaoMintParams = deserialize(&self_.data[1..])?;
            let dao_bulla = params.dao_bulla.inner();

            // Check the DAO bulla doesn't already exist
            let bulla_db = db_lookup(cid, DB_DAO_BULLAS)?;
            if db_contains_key(bulla_db, &serialize(&dao_bulla))? {
                msg!("DAO already exists: {:?}", dao_bulla);
                return Err(ContractError::Custom(1))
            }

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
            let dao_roots_db = db_lookup(cid, DB_DAO_MERKLE_ROOTS)?;
            if !db_contains_key(dao_roots_db, &serialize(&params.dao_merkle_root))? {
                msg!("Invalid DAO Merkle root: {}", params.dao_merkle_root);
                return Err(ContractError::Custom(3))
            }

            let proposal_db = db_lookup(cid, DB_PROPOSAL_BULLAS)?;
            // Make sure proposal doesn't already exist
            // Otherwise it will reset voting again
            if db_contains_key(proposal_db, &serialize(&params.proposal_bulla))? {
                msg!("Proposal already exists: {:?}", params.proposal_bulla);
                return Err(ContractError::Custom(4))
            }

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
            let proposal_votes_db = db_lookup(cid, DB_PROPOSAL_BULLAS)?;
            let Some(proposal_votes) = db_get(proposal_votes_db, &serialize(&params.proposal_bulla))? else {
                msg!("Invalid proposal {:?}", params.proposal_bulla);
                return Err(ContractError::Custom(4))
            };
            let mut proposal_votes: DaoBlindAggregateVote = deserialize(&proposal_votes)?;

            // Check the Merkle roots and nullifiers for the input coins are valid
            let money_roots_db = db_lookup(money_cid, MONEY_CONTRACT_COIN_ROOTS_TREE)?;
            let money_nullifier_db = db_lookup(money_cid, MONEY_CONTRACT_NULLIFIERS_TREE)?;
            let dao_vote_nulls_db = db_lookup(cid, DAO_VOTE_NULLS)?;

            let mut vote_nullifiers = vec![];

            for input in &params.inputs {
                if !db_contains_key(money_roots_db, &serialize(&input.merkle_root))? {
                    msg!("Invalid input Merkle root: {:?}", input.merkle_root);
                    return Err(ContractError::Custom(5))
                }

                if db_contains_key(money_nullifier_db, &serialize(&input.nullifier))? {
                    msg!("Coin is already spent");
                    return Err(ContractError::Custom(6))
                }

                // Prefix nullifier with proposal bulla so nullifiers from different proposals
                // don't interfere with each other.
                let null_key = serialize(&(params.proposal_bulla, input.nullifier));

                if vote_nullifiers.contains(&input.nullifier) ||
                    db_contains_key(dao_vote_nulls_db, &null_key)?
                {
                    msg!("Attempted double vote");
                    return Err(ContractError::Custom(7))
                }

                proposal_votes.all_vote_commit += input.vote_commit;
                vote_nullifiers.push(input.nullifier);
            }

            proposal_votes.yes_vote_commit += params.yes_vote_commit;

            let update = DaoVoteUpdate {
                proposal_bulla: params.proposal_bulla,
                proposal_votes,
                vote_nullifiers,
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
            assert!(call[0].data[0] == MoneyTransfer as u8);

            // 4. MoneyTransfer has exactly 2 outputs
            let mt_params: MoneyTransferParams = deserialize(&call[0].data[1..])?;
            assert!(mt_params.outputs.len() == 2);

            // ======
            // Checks
            // ======
            // 1. Check both coins in MoneyTransfer are equal to our coin_0, coin_1
            assert!(mt_params.outputs[0].coin.inner() == params.coin_0);
            assert!(mt_params.outputs[1].coin.inner() == params.coin_1);

            // 2. Sum of MoneyTransfer input value commits == our input value commit
            let mut input_valcoms = pallas::Point::identity();
            for input in mt_params.inputs {
                input_valcoms += input.value_commit;
            }
            assert!(input_valcoms == params.input_value_commit);

            // 3. Get the ProposalVote from DAO state
            let proposal_db = db_lookup(cid, DB_PROPOSAL_BULLAS)?;
            let Some(proposal_votes) = db_get(proposal_db, &serialize(&params.proposal))? else {
                msg!("Proposal {:?} not found in db", params.proposal);
                return Err(ContractError::Custom(1));
            };
            let proposal_votes: DaoBlindAggregateVote = deserialize(&proposal_votes)?;

            // 4. Check yes_vote_commit and all_vote_commit are the same as in BlindAggregateVote
            assert!(proposal_votes.yes_vote_commit == params.blind_total_vote.yes_vote_commit);
            assert!(proposal_votes.all_vote_commit == params.blind_total_vote.all_vote_commit);

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
            let dao_bulla = update.dao_bulla.inner();

            let info_db = db_lookup(cid, DB_INFO)?;
            let bulla_db = db_lookup(cid, DB_DAO_BULLAS)?;
            let roots_db = db_lookup(cid, DB_DAO_MERKLE_ROOTS)?;

            db_set(bulla_db, &serialize(&dao_bulla), &[])?;

            let node = MerkleNode::from(dao_bulla);
            merkle_add(info_db, roots_db, &serialize(&KEY_DAO_MERKLE_TREE), &[node])?;

            Ok(())
        }

        DaoFunction::Propose => {
            let update: DaoProposeUpdate = deserialize(&ix[1..])?;

            let proposal_vote_db = db_lookup(cid, DB_PROPOSAL_BULLAS)?;
            let pv = DaoBlindAggregateVote::default();

            db_set(proposal_vote_db, &serialize(&update.proposal_bulla), &serialize(&pv))?;

            Ok(())
        }

        DaoFunction::Vote => {
            let update: DaoVoteUpdate = deserialize(&ix[1..])?;

            // Perform this code:
            //   total_yes_vote_commit += update.yes_vote_commit
            //   total_all_vote_commit += update.all_vote_commit

            let proposal_vote_db = db_lookup(cid, DB_PROPOSAL_BULLAS)?;
            db_set(
                proposal_vote_db,
                &serialize(&update.proposal_bulla),
                &serialize(&update.proposal_votes),
            )?;

            // We are essentially doing: vote_nulls.append(update.nulls)

            let dao_vote_nulls_db = db_lookup(cid, DAO_VOTE_NULLS)?;

            for nullifier in update.vote_nullifiers {
                // Uniqueness is enforced for (proposal_bulla, nullifier)
                let key = serialize(&(update.proposal_bulla, nullifier));
                db_set(dao_vote_nulls_db, &key, &[])?;
            }

            Ok(())
        }

        DaoFunction::Exec => {
            let update: DaoExecUpdate = deserialize(&ix[1..])?;

            // Remove proposal from db
            let proposal_vote_db = db_lookup(cid, DB_PROPOSAL_BULLAS)?;
            db_del(proposal_vote_db, &serialize(&update.proposal))?;

            Ok(())
        }
    }
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;
    assert!(call_idx < call.len() as u32);

    let self_ = &call[call_idx as usize];

    match DaoFunction::try_from(self_.data[0])? {
        DaoFunction::Mint => {
            let params: DaoMintParams = deserialize(&self_.data[1..])?;

            let mut zk_public_values: Vec<(String, Vec<pallas::Base>)> = vec![];
            let signature_pubkeys: Vec<PublicKey> = vec![params.dao_pubkey];

            let (pub_x, pub_y) = params.dao_pubkey.xy();

            zk_public_values.push((
                DAO_CONTRACT_ZKAS_DAO_MINT_NS.to_string(),
                vec![pub_x, pub_y, params.dao_bulla.inner()],
            ));

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

            let mut all_vote_commit = pallas::Point::identity();

            for input in &params.inputs {
                signature_pubkeys.push(input.signature_public);
                all_vote_commit += input.vote_commit;

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
            let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

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

            let blind_vote = params.blind_total_vote;
            let yes_vote_coords = blind_vote.yes_vote_commit.to_affine().coordinates().unwrap();
            let all_vote_coords = blind_vote.all_vote_commit.to_affine().coordinates().unwrap();
            let input_value_coords = params.input_value_commit.to_affine().coordinates().unwrap();

            msg!("params.proposal: {:?}", params.proposal);
            zk_public_values.push((
                DAO_CONTRACT_ZKAS_DAO_EXEC_NS.to_string(),
                vec![
                    params.proposal,
                    params.coin_0,
                    params.coin_1,
                    *yes_vote_coords.x(),
                    *yes_vote_coords.y(),
                    *all_vote_coords.x(),
                    *all_vote_coords.y(),
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
