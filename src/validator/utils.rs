/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::sync::{Arc, LazyLock};

use darkfi_sdk::{
    crypto::{DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID},
    tx::TransactionHash,
};
use num_bigint::BigUint;
use parking_lot::Mutex;
use randomx::{RandomXCache, RandomXFlags, RandomXVM};
use tracing::info;

use crate::{
    blockchain::{BlockInfo, BlockchainOverlayPtr, Header},
    runtime::vm_runtime::{Runtime, TxLocalState},
    validator::{
        consensus::{Fork, Proposal},
        pow::PoWModule,
    },
    Error, Result,
};

/// Max 32 bytes integer, used in rank calculations.
/// Cached to avoid repeated allocation.
pub static MAX_32_BYTES: LazyLock<BigUint> = LazyLock::new(|| BigUint::from_bytes_le(&[0xFF; 32]));

/// Deploy DarkFi native wasm contracts to provided blockchain overlay.
///
/// If overlay already contains the contracts, it will just open the
/// necessary db and trees, and give back what it has. This means that
/// on subsequent runs, our native contracts will already be in a
/// deployed state, so what we actually do here is a redeployment. This
/// kind of operation should only modify the contract's state in case
/// it wasn't deployed before (meaning the initial run). Otherwise, it
/// shouldn't touch anything, or just potentially update the db schemas
/// or whatever is necessary. This logic should be handled in the init
/// function of the actual contract, so make sure the native contracts
/// handle this well.
pub async fn deploy_native_contracts(
    overlay: &BlockchainOverlayPtr,
    block_target: u32,
) -> Result<()> {
    info!(target: "validator::utils::deploy_native_contracts", "Deploying native WASM contracts");

    // The Money contract uses an empty payload to deploy itself.
    let money_contract_deploy_payload = vec![];

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

    // Grab last known block height to verify against next one.
    // If no blocks exist, we verify against genesis block height (0).
    let verifying_block_height = match overlay.lock().unwrap().last() {
        Ok((last_block_height, _)) => last_block_height + 1,
        Err(_) => 0,
    };

    for (call_idx, nc) in native_contracts.into_iter().enumerate() {
        info!(target: "validator::utils::deploy_native_contracts", "Deploying {} with ContractID {}", nc.0, nc.1);

        // Create tx-local state
        let mut tx_local_state = TxLocalState::new();
        tx_local_state.entry(nc.1).or_default();
        let tx_local_state = Arc::new(Mutex::new(tx_local_state));

        let mut runtime = Runtime::new(
            &nc.2[..],
            overlay.clone(),
            tx_local_state,
            nc.1,
            verifying_block_height,
            block_target,
            TransactionHash::none(),
            call_idx as u8,
        )?;

        runtime.deploy(&nc.3)?;

        info!(target: "validator::utils::deploy_native_contracts", "Successfully deployed {}", nc.0);
    }

    info!(target: "validator::utils::deploy_native_contracts", "Finished deployment of native WASM contracts");

    Ok(())
}

/// Verify provided header is valid for provided PoW module and compute
/// its rank.
/// Returns next mine difficulty, along with the computed rank.
///
/// Header's rank is the tuple of its squared mining target distance
/// from max 32 bytes int, along with its squared RandomX hash number
/// distance from max 32 bytes int.
/// Genesis block has rank (0, 0).
pub fn header_rank(module: &mut PoWModule, header: &Header) -> Result<(BigUint, BigUint, BigUint)> {
    // Grab next mine target and difficulty
    let (target, difficulty) = module.next_mine_target_and_difficulty()?;

    // Genesis header has rank 0
    if header.height == 0 {
        return Ok((difficulty, 0u64.into(), 0u64.into()))
    }

    // Verify hash is less than the expected mine target
    let out_hash = module.verify_block_target(header, &target)?;

    // Compute the squared mining target distance
    let target_distance = &*MAX_32_BYTES - target;
    let target_distance_sq = &target_distance * &target_distance;

    // Compute the output hash distance
    let hash_distance = &*MAX_32_BYTES - out_hash;
    let hash_distance_sq = &hash_distance * &hash_distance;

    Ok((difficulty, target_distance_sq, hash_distance_sq))
}

/// Compute a block's rank, assuming that its valid, based on provided
/// mining target.
///
/// Block's rank is the tuple of its squared mining target distance
/// from max 32 bytes int, along with its squared RandomX hash number
/// distance from max 32 bytes int. Genesis block has rank (0, 0).
pub fn block_rank(block: &BlockInfo, target: &BigUint) -> Result<(BigUint, BigUint)> {
    // Genesis block has rank 0
    if block.header.height == 0 {
        return Ok((0u64.into(), 0u64.into()))
    }

    // Compute the squared mining target distance
    let target_distance = &*MAX_32_BYTES - target;
    let target_distance_sq = &target_distance * &target_distance;

    // Setup RandomX verifier
    let flags = RandomXFlags::get_recommended_flags();
    let cache = RandomXCache::new(flags, block.header.previous.inner())?;
    let vm = RandomXVM::new(flags, Some(cache), None)?;

    // Compute the output hash distance
    let out_hash = vm.calculate_hash(block.hash().inner())?;
    let out_hash = BigUint::from_bytes_le(&out_hash);
    let hash_distance = &*MAX_32_BYTES - out_hash;
    let hash_distance_sq = &hash_distance * &hash_distance;

    Ok((target_distance_sq, hash_distance_sq))
}

/// Auxiliary function to calculate the middle value between provided
/// u64 numbers.
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

    if v.len().is_multiple_of(2) {
        return get_mid(v[n - 1], v[n])
    }
    v[n]
}

/// Given a proposal, find the index of a fork chain it extends, along
/// with the specific extended proposal index. Additionally, check that
/// proposal doesn't already exists in any fork chain.
pub fn find_extended_fork_index(forks: &[Fork], proposal: &Proposal) -> Result<(usize, usize)> {
    // Grab provided proposal hash
    let proposal_hash = proposal.hash;

    // Keep track of fork and proposal indexes
    let (mut fork_index, mut proposal_index) = (None, None);

    // Loop through all the forks
    for (f_index, fork) in forks.iter().enumerate() {
        // Traverse fork proposals sequence in reverse
        for (p_index, p_hash) in fork.proposals.iter().enumerate().rev() {
            // Check we haven't already seen that proposal
            if proposal_hash == *p_hash {
                return Err(Error::ProposalAlreadyExists)
            }

            // Check if proposal extends this fork
            if proposal.block.header.previous == *p_hash {
                (fork_index, proposal_index) = (Some(f_index), Some(p_index));
            }
        }
    }

    if let (Some(f_index), Some(p_index)) = (fork_index, proposal_index) {
        return Ok((f_index, p_index))
    }

    Err(Error::ExtendedChainIndexNotFound)
}

/// Auxiliary function to find best ranked fork.
///
/// The best ranked fork is the one with the highest sum of its blocks
/// squared mining target distances, from max 32 bytes int. In case of
/// a tie, the fork with the highest sum of its blocks squared RandomX
/// hash number distances, from max 32 bytes int, wins.
pub fn best_fork_index(forks: &[Fork]) -> Result<usize> {
    // Check if node has any forks
    if forks.is_empty() {
        return Err(Error::ForksNotFound)
    }

    // Find the best ranked forks
    let mut best = &BigUint::from(0u64);
    let mut indexes = vec![];
    for (f_index, fork) in forks.iter().enumerate() {
        let rank = &fork.targets_rank;

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

    // If a single best ranking fork exists, return it
    if indexes.len() == 1 {
        return Ok(indexes[0])
    }

    // Break tie using their hash distances rank
    let mut best_index = indexes[0];
    for index in &indexes[1..] {
        if forks[*index].hashes_rank > forks[best_index].hashes_rank {
            best_index = *index;
        }
    }

    Ok(best_index)
}

/// Auxiliary function to find worst ranked fork.
///
/// The worst ranked fork is the one with the lowest sum of its blocks
/// squared mining target distances, from max 32 bytes int. In case of
/// a tie, the fork with the lowest sum of its blocks squared RandomX
/// hash number distances, from max 32 bytes int, wins.
pub fn worst_fork_index(forks: &[Fork]) -> Result<usize> {
    // Check if node has any forks
    if forks.is_empty() {
        return Err(Error::ForksNotFound)
    }

    // Find the worst ranked forks
    let mut worst = &forks[0].targets_rank;
    let mut indexes = vec![0];
    for (f_index, fork) in forks[1..].iter().enumerate() {
        let rank = &fork.targets_rank;

        // Fork ranks higher that current worst
        if rank > worst {
            continue
        }

        // Fork has same rank as current worst
        if rank == worst {
            indexes.push(f_index + 1);
            continue
        }

        // Fork ranks lower that current worst
        worst = rank;
        indexes = vec![f_index + 1];
    }

    // If a single worst ranking fork exists, return it
    if indexes.len() == 1 {
        return Ok(indexes[0])
    }

    // Break tie using their hash distances rank
    let mut worst_index = indexes[0];
    for index in &indexes[1..] {
        if forks[*index].hashes_rank < forks[worst_index].hashes_rank {
            worst_index = *index;
        }
    }

    Ok(worst_index)
}
