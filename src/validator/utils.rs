/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_sdk::{
    crypto::{
        ecvrf::VrfProof, pasta_prelude::PrimeField, PublicKey, DAO_CONTRACT_ID,
        DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID,
    },
    pasta::{group::ff::FromUniformBytes, pallas},
};
use darkfi_serial::{serialize_async, AsyncDecodable};
use log::info;
use smol::io::Cursor;

use crate::{
    blockchain::{BlockInfo, BlockchainOverlayPtr},
    error::TxVerifyFailed,
    runtime::vm_runtime::Runtime,
    tx::Transaction,
    validator::consensus::{Fork, Proposal},
    Error, Result,
};

/// Deploy DarkFi native wasm contracts to provided blockchain overlay.
/// If overlay already contains the contracts, it will just open the
/// necessary db and trees, and give back what it has. This means that
/// on subsequent runs, our native contracts will already be in a deployed
/// state, so what we actually do here is a redeployment. This kind of
/// operation should only modify the contract's state in case it wasn't
/// deployed before (meaning the initial run). Otherwise, it shouldn't
/// touch anything, or just potentially update the db schemas or whatever
/// is necessary. This logic should be handled in the init function of
/// the actual contract, so make sure the native contracts handle this well.
pub async fn deploy_native_contracts(
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u64,
    faucet_pubkeys: &Vec<PublicKey>,
) -> Result<()> {
    info!(target: "validator::utils::deploy_native_contracts", "Deploying native WASM contracts");

    // The faucet pubkeys are pubkeys which are allowed to create clear inputs
    // in the Money contract.
    let money_contract_deploy_payload = serialize_async(faucet_pubkeys).await;

    // The DAO contract uses an empty payload to deploy itself.
    let dao_contract_deploy_payload = vec![];

    // The Deployooor contract uses an empty payload to deploy itself.
    let deployooor_contract_deploy_payload = vec![];

    let native_contracts = vec![
        (
            "Money Contract",
            *MONEY_CONTRACT_ID,
            include_bytes!("../contract/money/darkfi_money_contract.wasm").to_vec(),
            money_contract_deploy_payload,
        ),
        (
            "DAO Contract",
            *DAO_CONTRACT_ID,
            include_bytes!("../contract/dao/darkfi_dao_contract.wasm").to_vec(),
            dao_contract_deploy_payload,
        ),
        (
            "Deployooor Contract",
            *DEPLOYOOOR_CONTRACT_ID,
            include_bytes!("../contract/deployooor/darkfi_deployooor_contract.wasm").to_vec(),
            deployooor_contract_deploy_payload,
        ),
    ];

    for nc in native_contracts {
        info!(target: "validator::utils::deploy_native_contracts", "Deploying {} with ContractID {}", nc.0, nc.1);

        let mut runtime = Runtime::new(&nc.2[..], overlay.clone(), nc.1, verifying_block_height)?;

        runtime.deploy(&nc.3)?;

        info!(target: "validator::utils::deploy_native_contracts", "Successfully deployed {}", nc.0);
    }

    info!(target: "validator::utils::deploy_native_contracts", "Finished deployment of native WASM contracts");

    Ok(())
}

/// Compute a block's rank, assuming the its valid.
/// Genesis block has rank 0.
/// First 2 blocks rank is equal to their nonce, since their previous
/// previous block producer doesn't exist or have a VRF.
pub async fn block_rank(block: &BlockInfo, previous_previous: &BlockInfo) -> Result<u64> {
    // Genesis block has rank 0
    if block.header.height == 0 {
        return Ok(0)
    }

    // Compute nonce u64
    let mut nonce = [0u8; 8];
    nonce.copy_from_slice(&block.header.nonce.to_repr()[..8]);
    let nonce = u64::from_be_bytes(nonce);

    // First 2 blocks have rank equal to their nonce
    if block.header.height < 3 {
        return Ok(nonce)
    }

    // Extract VRF proof from the previous previous producer transaction
    let tx = previous_previous.txs.last().unwrap();
    let data = &tx.calls[0].data.data;
    let mut decoder = Cursor::new(&data);
    // PoW uses MoneyPoWRewardParamsV1
    decoder.set_position(563);
    let vrf_proof: VrfProof = AsyncDecodable::decode_async(&mut decoder).await?;

    // Compute VRF u64
    let mut vrf = [0u8; 64];
    vrf[..blake3::OUT_LEN].copy_from_slice(vrf_proof.hash_output().as_bytes());
    let vrf_pallas = pallas::Base::from_uniform_bytes(&vrf);
    let mut vrf = [0u8; 8];
    vrf.copy_from_slice(&vrf_pallas.to_repr()[..8]);
    let vrf = u64::from_be_bytes(vrf);

    // Finally, compute the rank
    let rank = nonce % vrf;

    Ok(rank)
}

/// Auxiliary function to calculate the middle value between provided u64 numbers
pub fn get_mid(a: u64, b: u64) -> u64 {
    (a / 2) + (b / 2) + ((a - 2 * (a / 2)) + (b - 2 * (b / 2))) / 2
}

/// Auxiliary function to calculate the median of a given `Vec<u64>`.
/// The function sorts the vector internally.
pub fn median(mut v: Vec<u64>) -> u64 {
    if v.len() == 1 {
        return v[0]
    }

    let n = v.len() / 2;
    v.sort_unstable();

    if v.len() % 2 == 0 {
        v[n]
    } else {
        get_mid(v[n - 1], v[n])
    }
}

/// Auxiliary function to calculate the total amount of minted tokens in provided
/// genesis transactions set. This includes both staked and normal tokens.
/// If a non-genesis transaction is found, execution fails.
/// Set must also include the genesis transaction(empty) at first position.
pub async fn genesis_txs_total(txs: &[Transaction]) -> Result<u64> {
    let mut total = 0;

    if txs.is_empty() {
        return Ok(total)
    }

    if txs[0] != Transaction::default() {
        return Err(TxVerifyFailed::ErroneousTxs(vec![txs[0].clone()]).into())
    }

    // Iterate transactions, exluding producer(first) one
    for tx in &txs[1..] {
        // Transaction must contain a single Consensus::GenesisStake (0x00)
        // or Money::GenesisMint (0x01) call
        if tx.calls.len() != 1 {
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }
        let call = &tx.calls[0];
        let data = &call.data.data;
        let function = data[0];
        if call.data.contract_id != *MONEY_CONTRACT_ID || function != 0x01_u8 {
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }

        // Extract transaction input value.
        // Consensus::GenesisStake uses ConsensusGenesisStakeParamsV1, while
        // Money::GenesisMint uses MoneyGenesisMintParamsV1. Both params structs
        // have the value at same position (1).
        let position = 1;
        let mut decoder = Cursor::new(&data);
        decoder.set_position(position);
        let value: u64 = AsyncDecodable::decode_async(&mut decoder).await?;

        total += value;
    }

    Ok(total)
}

/// Given a proposal, find the index of the fork chain it extends, along with the specific
/// extended proposal index. Additionally, check that proposal doesn't already exists in any
/// fork chain.
pub fn find_extended_fork_index(forks: &[Fork], proposal: &Proposal) -> Result<(usize, usize)> {
    // Grab provided proposal hash
    let proposal_hash = proposal.block.hash()?;

    // Keep track of fork and proposal indexes
    let (mut fork_index, mut proposal_index) = (None, None);

    // Loop through all the forks
    for (f_index, fork) in forks.iter().enumerate() {
        // Traverse fork proposals sequence in reverse
        for (p_index, p_hash) in fork.proposals.iter().enumerate().rev() {
            // Check we haven't already seen that proposal
            if &proposal_hash == p_hash {
                return Err(Error::ProposalAlreadyExists)
            }

            // Check if proposal extends this fork
            if &proposal.block.header.previous == p_hash {
                // Proposal must only extend a single fork
                if fork_index.is_some() {
                    return Err(Error::ProposalAlreadyExists)
                }
                (fork_index, proposal_index) = (Some(f_index), Some(p_index));
            }
        }
    }

    if let (Some(f_index), Some(p_index)) = (fork_index, proposal_index) {
        return Ok((f_index, p_index))
    }

    Err(Error::ExtendedChainIndexNotFound)
}

/// Auxiliary function to find best ranked forks indexes.
pub fn best_forks_indexes(forks: &[Fork]) -> Result<Vec<usize>> {
    // Check if node has any forks
    if forks.is_empty() {
        return Err(Error::ForksNotFound)
    }

    // Find the best ranked forks
    let mut best = 0;
    let mut indexes = vec![];
    for (f_index, fork) in forks.iter().enumerate() {
        let rank = fork.rank;

        // Fork ranks lower that current best
        if rank < best {
            continue
        }

        // Fork has same rank as current best
        if rank == best {
            indexes.push(f_index);
            continue
        }

        // Fork ranks higher that current best
        best = rank;
        indexes = vec![f_index];
    }

    Ok(indexes)
}
