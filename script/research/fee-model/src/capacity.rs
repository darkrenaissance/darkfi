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

//! Validator block capacity benchmark
//!
//! Measures per-tx gas and per-tx wall-clock time (verify + apply) for a
//! set of prebuilt-transaction scenarios. Each scenario builds a batch of
//! transactions and runs it through the validator. Output is raw
//! measurements (gas_per_tx, secs_per_tx) tagged with machine info.
//!
//! Scenarios:
//!
//! | scenario                | shape                                                     |
//! |-------------------------|-----------------------------------------------------------|
//! | transfer_simple         | transfer 1-in/2-out                                       |
//! | transfer_20in_2out      | transfer 20-in/2-out                                      |
//! | transfer_1in_20out      | transfer 1-in/20-out                                      |
//! | dao_propose_20recip     | DAO propose, 20 recipients                                |
//! | dao_exec_20recip        | DAO exec, 20 recipients                                   |
//! | dao_vote                | DAO vote                                                  |
//! | otc_swap                | OTC swap                                                  |
//! | token_mint              | token mint                                                |
//! | dao_mint                | DAO mint                                                  |
//! | deploy_512kb            | deploy 512 KiB WASM                                       |
//! | deploy_1024kb           | deploy 1024 KiB WASM                                      |
//! | mixed                   | ~78% transfers, 10% votes, 6% execs, 4% mints, 2% deploys |
//!
//! verification_secs is the median of 3 verify-only (write=false) runs;
//! apply_secs is derived from one write=true pass as
//! max(0, apply_total - verification_secs). Verify-only trials reuse the
//! same base state because write=true spends the prebuilt coins.
//!
//! Usage:
//!   make capacity
//!   MONEY_WASM_PATH=/path/to.wasm make capacity   # bundled/container WASM

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    validator::consensus::BLOCK_GAS_LIMIT,
    Result,
};
use darkfi_contract_test_harness::{Holder, TestHarness};
use darkfi_dao_contract::model::Dao as DaoModel;
use darkfi_money_contract::{
    client::{
        fee_v1::FEE_CALL_GAS,
        transfer_v1::{TransferCallBuilder, TransferCallInput},
        OwnCoin,
    },
    model::{CoinAttributes, TokenId, DARK_TOKEN_ID},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        contract_id::MONEY_CONTRACT_ID,
        pasta_prelude::*,
        util::fp_mod_fv,
        BaseBlind, Blind, FuncId, FuncRef, Keypair, ScalarBlind, DAO_CONTRACT_ID,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use serde::Serialize;
use std::time::Instant;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Helpers (capacity-only: WASM padding, machine-info detection, merkle filler)
// ---------------------------------------------------------------------------

/// Convert a `pallas::Base` field element to a `u64`, erroring if the value
/// does not fit in the low 8 bytes.
fn fp_to_u64(f: pallas::Base) -> Result<u64> {
    let repr = f.to_repr();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&repr.as_ref()[..8]);
    for b in &repr.as_ref()[8..] {
        if *b != 0 {
            return Err(darkfi::Error::Custom("fp_to_u64 overflow".into()));
        }
    }
    Ok(u64::from_le_bytes(bytes))
}

/// DAO keys bundle for convenience.
struct DaoKeys {
    notes: Keypair,
    proposer: Keypair,
    proposals: Keypair,
    votes: Keypair,
    exec: Keypair,
    early_exec: Keypair,
}

/// Encode a `u64` as unsigned LEB128 (used by the WASM binary format).
fn encode_uleb128(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

/// Build a WASM custom section (section id 0x00) with the given name and
/// payload.  Custom sections are ignored by the WASM runtime during
/// execution but are still parsed by `wasmparser::validate`, so the
/// deployooor contract accepts them.
fn build_custom_section(name: &str, data: &[u8]) -> Vec<u8> {
    let name_bytes = name.as_bytes();
    let name_len_bytes = encode_uleb128(name_bytes.len() as u64);
    let payload_len = name_len_bytes.len() + name_bytes.len() + data.len();
    let payload_len_bytes = encode_uleb128(payload_len as u64);

    let mut section = Vec::with_capacity(1 + payload_len_bytes.len() + payload_len);
    section.push(0x00); // custom section id
    section.extend_from_slice(&payload_len_bytes);
    section.extend_from_slice(&name_len_bytes);
    section.extend_from_slice(name_bytes);
    section.extend_from_slice(data);
    section
}

/// Pad `wasm` to approximately `target_bytes` total by appending a single
/// custom data section named ``padding``.  If `target_bytes` is not larger
/// than the input, the original bytes are returned unchanged.
fn pad_wasm_to_size(wasm: &[u8], target_bytes: usize) -> Vec<u8> {
    if target_bytes <= wasm.len() {
        return wasm.to_vec();
    }
    let name = "padding";
    let overhead = 1 // section id
        + encode_uleb128(0).len()
        + encode_uleb128(name.len() as u64).len()
        + name.len();
    let fill = target_bytes - wasm.len() - overhead;
    let mut result = wasm.to_vec();
    result.extend_from_slice(&build_custom_section(name, &vec![0u8; fill]));
    result
}

/// Hardware information embedded in the capacity benchmark output so that
/// runs from different machines can be grouped during analysis.
#[derive(Clone, Debug, Serialize)]
struct MachineInfo {
    cpu_model: String,
    physical_cores: usize,
    logical_cores: usize,
    ram_gb: u64,
    disk_type: String,
}

/// Detect machine hardware info from `/proc` and `/sys`.  Falls back to
/// `"unknown"` for any field that cannot be read.
fn detect_machine_info() -> MachineInfo {
    MachineInfo {
        cpu_model: detect_cpu_model().unwrap_or_else(|| "unknown".into()),
        physical_cores: detect_physical_cores().unwrap_or(0),
        logical_cores: detect_logical_cores().unwrap_or(0),
        ram_gb: detect_ram_gb().unwrap_or(0),
        disk_type: detect_disk_type().unwrap_or_else(|| "unknown".into()),
    }
}

fn detect_cpu_model() -> Option<String> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    for line in cpuinfo.lines() {
        if let Some(rest) = line.strip_prefix("model name") {
            if let Some(val) = rest.split(':').nth(1) {
                return Some(val.trim().to_string());
            }
        }
    }
    None
}

/// Count physical cores as the number of distinct `(physical id, core id)`
/// pairs in `/proc/cpuinfo`. This is correct on multi-socket and multi-core
/// systems without assuming a single socket. Returns `None` when the kernel
/// does not expose topology (some VMs/containers).
fn detect_physical_cores() -> Option<usize> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let mut cores: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut cur_phys: Option<String> = None;
    for line in cpuinfo.lines() {
        if let Some(rest) = line.strip_prefix("physical id") {
            cur_phys = rest.split(':').nth(1).map(|s| s.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("core id") {
            let cur_core = rest.split(':').nth(1).map(|s| s.trim().to_string());
            if let (Some(p), Some(c)) = (&cur_phys, &cur_core) {
                cores.insert((p.clone(), c.clone()));
            }
        }
    }
    if cores.is_empty() { None } else { Some(cores.len()) }
}

fn detect_logical_cores() -> Option<usize> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    Some(cpuinfo.lines().filter(|l| l.starts_with("processor")).count())
}

fn detect_ram_gb() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb / 1_024 / 1024);
        }
    }
    None
}

/// Best-effort disk type detection.  Checks the rotational flag of the
/// block device backing the working directory.  Returns `"ssd"` (rota=0),
/// `"hdd"` (rota=1), or `"unknown"`.
fn detect_disk_type() -> Option<String> {
    // Walk /sys/block and find the device whose name is a prefix of the
    // root filesystem's source.  This is rough but good enough for tagging.
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let root_source = mounts.lines().find(|l| l.split_whitespace().nth(1) == Some("/"))?;
    let dev_path = root_source.split_whitespace().next()?;
    let dev_name = dev_path.rsplit('/').next()?;

    // Strip the trailing partition index to recover the base block device.
    // NVMe and MMC use a 'p' separator before the partition number
    // (nvme0n1p3 -> nvme0n1, mmcblk0p1 -> mmcblk0); SATA/virtio use bare
    // digits (sda3 -> sda, vda1 -> vda).
    let base_name = if dev_name.starts_with("nvme") || dev_name.starts_with("mmcblk") {
        if let Some(p_idx) = dev_name.rfind('p') {
            let after = &dev_name[p_idx + 1..];
            if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
                &dev_name[..p_idx]
            } else {
                dev_name
            }
        } else {
            dev_name
        }
    } else {
        dev_name.trim_end_matches(|c: char| c.is_ascii_digit())
    };

    let rota_path = format!("/sys/block/{}/queue/rotational", base_name);
    let rota = std::fs::read_to_string(&rota_path).ok()?;
    let rota = rota.trim();
    match rota {
        "0" => Some("ssd".into()),
        "1" => Some("hdd".into()),
        _ => None,
    }
}

/// Pre-populate every holder's Merkle tree with `count` filler coins via
/// batched genesis mints. Coins are minted to `holder`, but every holder's
/// wallet verifies the tx and appends to its own tree, so all trees grow
/// uniformly. Genesis mint always runs at block 0 (enforced by the
/// contract).
async fn pre_populate_merkle_tree(
    th: &mut TestHarness,
    holder: &Holder,
    count: usize,
) -> Result<()> {
    // Each genesis mint output adds ~3M gas (31M base + 3M/output).
    // With 128 outputs per batch we stay well under CONTRACT_GAS_LIMIT (800M).
    const BATCH_SIZE: usize = 128;
    let batches = count / BATCH_SIZE;
    let remainder = count % BATCH_SIZE;

    for i in 0..batches {
        let amounts: Vec<u64> = vec![1_000_000_000u64; BATCH_SIZE];
        th.genesis_mint_to_all(holder, &amounts, 0).await?;
        if (i + 1) % 4 == 0 {
            debug!("  filler batch {}/{} ({} coins total)", i + 1, batches, (i + 1) * BATCH_SIZE);
        }
    }

    if remainder > 0 {
        let amounts: Vec<u64> = vec![1_000_000_000u64; remainder];
        th.genesis_mint_to_all(holder, &amounts, 0).await?;
    }

    Ok(())
}

const TRANSFER_AMOUNT: u64 = 1_000;
const TRANSFER_COIN_VALUE: u64 = 10_000;

// Default WASM path (overridable via MONEY_WASM_PATH env var). The
// containerized run sets this to the bundled WASM location.
const DEFAULT_MONEY_WASM_PATH: &str = "../../../src/contract/money/darkfi_money_contract.wasm";

fn money_wasm_path() -> String {
    std::env::var("MONEY_WASM_PATH").unwrap_or_else(|_| DEFAULT_MONEY_WASM_PATH.into())
}

/// Number of verify-only trials run per scenario; the median `total_secs`
/// is reported as `verification_secs`, reducing single-shot scheduling
/// noise. `write=false` is required so each trial verifies against the
/// same base state (a `write=true` trial would spend the prebuilt txs'
/// coins and break the next trial).
const TRIALS: usize = 3;

// ---------------------------------------------------------------------------
// Scenario definitions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum OpKind {
    Transfer,       // 1 input, 2 outputs (baseline)
    TransferNIn,    // N inputs, 2 outputs
    TransferNOut,   // 1 input, N outputs
    DaoPropose,     // N recipients
    DaoExec,        // N recipients (pre-voted)
    DaoVote,        // single-shape
    OtcSwap,        // single-shape
    TokenMint,      // single-shape
    DaoMint,        // single-shape (deploys N distinct DAOs)
    Deploy,         // N KB WASM
    Mixed,          // mixed workload
}

#[derive(Clone, Debug)]
struct Scenario {
    name: String,
    op_kind: OpKind,
    batch_size: usize,
    /// Complexity dimensions (interpreted per op_kind)
    transfer_inputs: usize,
    transfer_outputs: usize,
    dao_recipients: usize,
    deploy_kb: usize,
    prepopulate_coins: usize,
}

fn all_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "transfer_simple".into(),
            op_kind: OpKind::Transfer,
            batch_size: 50,
            transfer_inputs: 1,
            transfer_outputs: 2,
            dao_recipients: 0,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "transfer_20in_2out".into(),
            op_kind: OpKind::TransferNIn,
            batch_size: 10,
            transfer_inputs: 20,
            transfer_outputs: 2,
            dao_recipients: 0,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "transfer_1in_20out".into(),
            op_kind: OpKind::TransferNOut,
            batch_size: 10,
            transfer_inputs: 1,
            transfer_outputs: 20,
            dao_recipients: 0,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "dao_propose_20recip".into(),
            op_kind: OpKind::DaoPropose,
            batch_size: 10,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 20,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "dao_exec_20recip".into(),
            op_kind: OpKind::DaoExec,
            batch_size: 10,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 20,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "dao_vote".into(),
            op_kind: OpKind::DaoVote,
            batch_size: 10,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 1,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "otc_swap".into(),
            op_kind: OpKind::OtcSwap,
            batch_size: 20,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 0,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "token_mint".into(),
            op_kind: OpKind::TokenMint,
            batch_size: 50,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 0,
            deploy_kb: 0,
            prepopulate_coins: 256,
        },
        Scenario {
            name: "dao_mint".into(),
            op_kind: OpKind::DaoMint,
            batch_size: 20,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 0,
            deploy_kb: 0,
            prepopulate_coins: 0,
        },
        Scenario {
            name: "deploy_512kb".into(),
            op_kind: OpKind::Deploy,
            batch_size: 5,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 0,
            deploy_kb: 512,
            prepopulate_coins: 0,
        },
        Scenario {
            name: "deploy_1024kb".into(),
            op_kind: OpKind::Deploy,
            batch_size: 3,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 0,
            deploy_kb: 1024,
            prepopulate_coins: 0,
        },
        Scenario {
            name: "mixed".into(),
            op_kind: OpKind::Mixed,
            batch_size: 51,
            transfer_inputs: 0,
            transfer_outputs: 0,
            dao_recipients: 1,
            deploy_kb: 512,
            prepopulate_coins: 256,
        },
    ]
}

// ---------------------------------------------------------------------------
// Mixed workload definition (for mixed scenario)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct WorkloadMix {
    transfers: usize,
    dao_votes: usize,
    dao_execs: usize,
    token_mints: usize,
    deployments: usize,
}

impl WorkloadMix {
    fn total(&self) -> usize {
        self.transfers + self.dao_votes + self.dao_execs + self.token_mints + self.deployments
    }
}

impl WorkloadMix {
    fn mixed() -> Self {
        // 51 txs: ~78% transfers, ~10% DAO votes, ~6% DAO execs,
        // ~4% token mints, ~2% deployments.
        WorkloadMix { transfers: 40, dao_votes: 5, dao_execs: 3, token_mints: 2, deployments: 1 }
    }
}

// ---------------------------------------------------------------------------
// DAO context setup
// ---------------------------------------------------------------------------

struct DaoContext {
    dao_obj: DaoModel,
    dao_keys: DaoKeys,
    gov_token_id: TokenId,
    proposals: Vec<(darkfi_dao_contract::model::DaoProposal, Vec<CoinAttributes>)>,
    vote_data: Vec<(u64, u64, ScalarBlind, ScalarBlind)>,
}

/// DAO setup parameters.  Different scenarios need different combinations:
///
/// - `dao_propose`: `num_gov_coins = batch_size`, no proposals, no pre-votes
/// - `dao_vote`: `num_gov_coins = 2*batch_size` (stakes + vote coins),
///    `num_proposals = batch_size`, no pre-votes
/// - `dao_exec`: `num_gov_coins = 2*batch_size` (stakes + setup votes),
///    `num_proposals = batch_size`, `num_pre_votes = batch_size`
struct DaoSetup {
    num_gov_coins: usize,
    num_proposals: usize,
    num_pre_votes: usize,
    recipients_per_proposal: usize,
}

/// Set up a DAO with gov tokens, proposals, and optional pre-votes.
///
/// Each `token_mint_with_blind_to_all` call executes and gives Alice one gov
/// token coin.  Each `dao_propose_transfer_to_all` executes and consumes one
/// gov coin (stake).  Each `dao_vote_to_all` executes and consumes one gov
/// coin.  After setup, Alice retains `num_gov_coins - num_proposals -
/// num_pre_votes` gov coins for building vote/propose txs.
async fn setup_dao_context(
    th: &mut TestHarness,
    setup: &DaoSetup,
    block_height: u32,
) -> Result<(DaoContext, u32)> {
    let gov_token_blind = BaseBlind::random(&mut OsRng);
    let gov_token_id = th.derive_token_id(&Holder::Alice, gov_token_blind);

    let dao_keys = DaoKeys {
        notes: th.wallet(&Holder::Dao).keypair.clone(),
        proposer: Keypair::random(&mut OsRng),
        proposals: Keypair::random(&mut OsRng),
        votes: Keypair::random(&mut OsRng),
        exec: Keypair::random(&mut OsRng),
        early_exec: Keypair::random(&mut OsRng),
    };

    let dao_obj = DaoModel {
        proposer_limit: 20_000_000_000,
        quorum: 10_000_000_000,
        early_exec_quorum: 10_000_000_000,
        approval_ratio_quot: 67,
        approval_ratio_base: 100,
        gov_token_id,
        notes_public_key: dao_keys.notes.public,
        proposer_public_key: dao_keys.proposer.public,
        proposals_public_key: dao_keys.proposals.public,
        votes_public_key: dao_keys.votes.public,
        exec_public_key: dao_keys.exec.public,
        early_exec_public_key: dao_keys.early_exec.public,
        bulla_blind: Blind::random(&mut OsRng),
    };

    // Fund DAO treasury at genesis (needed for exec transfers).
    // Create one coin per exec tx so each can spend a distinct treasury
    // coin (otherwise batch verification fails with DuplicateNullifier).
    let dao_spend_hook = FuncRef {
        contract_id: *DAO_CONTRACT_ID,
        func_code: darkfi_dao_contract::DaoFunction::Exec as u8,
    }
    .to_func_id();
    let dao_bulla = dao_obj.to_bulla();
    let num_treasury_coins = setup.num_pre_votes.max(1);
    let treasury_per_coin = 500_000_000_000u64;
    let treasury_amounts = vec![treasury_per_coin; num_treasury_coins];
    let (genesis_tx, genesis_params) = th
        .genesis_mint(
            &Holder::Dao,
            &treasury_amounts,
            Some(dao_spend_hook),
            Some(dao_bulla.inner()),
        )
        .await?;
    th.genesis_mint_to_all_with(genesis_tx, &genesis_params, 0).await?;

    let mut bh = block_height;

    // DAO mint
    th.dao_mint_to_all(
        &Holder::Alice,
        &dao_obj,
        &dao_keys.notes.secret,
        &dao_keys.proposer.secret,
        &dao_keys.proposals.secret,
        &dao_keys.votes.secret,
        &dao_keys.exec.secret,
        &dao_keys.early_exec.secret,
        bh,
    )
    .await?;
    bh += 1;

    // Mint gov token coins (each call executes and gives Alice one coin).
    // The value must exceed the DAO's proposer_limit (20B) to be valid as
    // a propose stake.
    for _ in 0..setup.num_gov_coins {
        th.token_mint_with_blind_to_all(
            100_000_000_000u64,
            &Holder::Alice,
            &Holder::Alice,
            gov_token_blind,
            bh,
        )
        .await?;
        bh += 1;
    }

    // Create proposals
    let recipient_holders: Vec<Holder> =
        th.holder_keys.iter().filter(|h| **h != Holder::Dao).cloned().collect();

    let mut proposals = Vec::with_capacity(setup.num_proposals);
    let mut vote_data = Vec::with_capacity(setup.num_pre_votes);

    for i in 0..setup.num_proposals {
        let coin_attrs: Vec<CoinAttributes> = (0..setup.recipients_per_proposal)
            .map(|j| {
                let holder = recipient_holders[(i * setup.recipients_per_proposal + j) % recipient_holders.len()];
                CoinAttributes {
                    public_key: th.wallet(&holder).keypair.public,
                    value: 1_000_000_000u64,
                    token_id: *DARK_TOKEN_ID,
                    spend_hook: FuncId::none(),
                    user_data: pallas::Base::ZERO,
                    blind: Blind::random(&mut OsRng),
                }
            })
            .collect();

        let proposal = th
            .dao_propose_transfer_to_all(
                &Holder::Alice,
                &coin_attrs,
                pallas::Base::ZERO,
                &dao_obj,
                &dao_keys.proposer.secret,
                bh,
                100,
            )
            .await?;
        bh += 1;

        // Pre-vote if needed
        if i < setup.num_pre_votes {
            let vote = th.dao_vote_to_all(&Holder::Alice, true, &dao_obj, &proposal, bh).await?;
            bh += 1;

            let note = vote.note.decrypt_unsafe(&dao_keys.votes.secret).unwrap();
            let vote_option = note[0];
            let yes_blind = Blind(fp_mod_fv(note[1]));
            let all_vote_value_raw = note[2];
            let all_blind = Blind(fp_mod_fv(note[3]));

            let all_vote_value = fp_to_u64(all_vote_value_raw).unwrap();
            let vote_option_val = fp_to_u64(vote_option).unwrap();
            let yes_vote_value = if vote_option_val == 1 { all_vote_value } else { 0 };

            vote_data.push((yes_vote_value, all_vote_value, yes_blind, all_blind));
        }

        proposals.push((proposal, coin_attrs));
    }

    Ok((DaoContext { dao_obj, dao_keys, gov_token_id, proposals, vote_data }, bh))
}

// ---------------------------------------------------------------------------
// Transaction builders (prebuild txs, not timed)
// ---------------------------------------------------------------------------

/// Build `n` transfer transactions (1 input, 2 outputs).  Each spends a
/// distinct genesis coin.
async fn build_transfer_txs(
    th: &mut TestHarness,
    n: usize,
    block_height: u32,
) -> Result<Vec<Transaction>> {
    let amounts = vec![TRANSFER_COIN_VALUE; n];
    th.genesis_mint_to_all(&Holder::Alice, &amounts, 0).await?;

    let mut coins = th.coins_by_token(&Holder::Alice, *DARK_TOKEN_ID);
    coins.retain(|c| c.note.value == TRANSFER_COIN_VALUE);
    coins.truncate(n);

    let mut txs = Vec::with_capacity(n);
    for coin in coins.into_iter().take(n) {
        let (tx, _params, _spent) = th
            .transfer(
                TRANSFER_AMOUNT,
                &Holder::Alice,
                &Holder::Bob,
                &[coin],
                *DARK_TOKEN_ID,
                block_height,
                false,
            )
            .await?;
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` transfer transactions with `num_inputs` inputs each and 2
/// outputs.  Pre-mints `n * num_inputs` distinct genesis coins so each tx
/// gets a fresh set of inputs.
async fn build_transfer_n_input_txs(
    th: &mut TestHarness,
    n: usize,
    num_inputs: usize,
    block_height: u32,
) -> Result<Vec<Transaction>> {
    const COIN_VALUE: u64 = 10_000_000_000u64;
    let total_coins = n * num_inputs;
    let amounts = vec![COIN_VALUE; total_coins];
    th.genesis_mint_to_all(&Holder::Alice, &amounts, 0).await?;

    let mut coins = th.coins_by_token(&Holder::Alice, *DARK_TOKEN_ID);
    coins.retain(|c| c.note.value == COIN_VALUE);
    coins.truncate(total_coins);

    let amount = num_inputs as u64 * COIN_VALUE - 1;

    let mut txs = Vec::with_capacity(n);
    for i in 0..n {
        let input_slice = &coins[i * num_inputs..(i + 1) * num_inputs];
        let (tx, _params, _spent) = th
            .transfer(
                amount,
                &Holder::Alice,
                &Holder::Bob,
                input_slice,
                *DARK_TOKEN_ID,
                block_height,
                false,
            )
            .await?;
        txs.push(tx);
    }
    Ok(txs)
}

/// Build a single transfer tx with 1 input and `num_outputs` outputs using
/// `TransferCallBuilder` directly (the wallet `transfer()` helper only does
/// 2-3 outputs). Returns the tx without executing.
fn build_transfer_n_output_tx(
    th: &TestHarness,
    sender: &Holder,
    recipients: &[Holder],
    input_coin: &OwnCoin,
    token_id: TokenId,
    output_value: u64,
    num_outputs: usize,
) -> Result<Transaction> {
    let wallet = th.wallet(sender);
    let (mint_pk, mint_zkbin) = th.proving_keys.get(MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
    let (burn_pk, burn_zkbin) = th.proving_keys.get(MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();

    let inputs = vec![TransferCallInput {
        coin: input_coin.clone(),
        merkle_path: wallet.money_merkle_tree.witness(input_coin.leaf_position, 0).unwrap(),
        user_data_blind: Blind::random(&mut OsRng),
    }];

    let outputs: Vec<CoinAttributes> = (0..num_outputs)
        .map(|i| {
            let recipient = recipients[i % recipients.len()];
            CoinAttributes {
                public_key: th.wallet(&recipient).keypair.public,
                value: output_value,
                token_id,
                spend_hook: FuncId::none(),
                user_data: pallas::Base::ZERO,
                blind: Blind::random(&mut OsRng),
            }
        })
        .collect();

    let builder = TransferCallBuilder {
        clear_inputs: vec![],
        inputs,
        outputs,
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
    };

    let (params, secrets) = builder.build()?;
    let mut data = vec![MoneyFunction::TransferV1 as u8];
    params.encode(&mut data)?;
    let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: secrets.proofs }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&secrets.signature_secrets)?;
    tx.signatures = vec![sigs];
    Ok(tx)
}

/// Build `n` transfer transactions with 1 input and `num_outputs` outputs
/// each.  Pre-mints `n` distinct genesis coins so each tx gets a fresh input.
async fn build_transfer_n_output_txs(
    th: &mut TestHarness,
    n: usize,
    num_outputs: usize,
    _block_height: u32,
) -> Result<Vec<Transaction>> {
    const OUTPUT_VALUE: u64 = 1_000_000_000u64;
    let total_value = num_outputs as u64 * OUTPUT_VALUE;
    let amounts = vec![total_value; n];
    th.genesis_mint_to_all(&Holder::Alice, &amounts, 0).await?;

    let mut coins = th.coins_by_token(&Holder::Alice, *DARK_TOKEN_ID);
    coins.retain(|c| c.note.value == total_value);
    coins.truncate(n);

    let recipients = [Holder::Alice, Holder::Bob];
    let mut txs = Vec::with_capacity(n);
    for coin in coins.into_iter().take(n) {
        let tx = build_transfer_n_output_tx(
            th,
            &Holder::Alice,
            &recipients,
            &coin,
            *DARK_TOKEN_ID,
            OUTPUT_VALUE,
            num_outputs,
        )?;
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` token mint transactions.  Each mints a fresh token.
async fn build_token_mint_txs(
    th: &mut TestHarness,
    n: usize,
    block_height: u32,
) -> Result<Vec<Transaction>> {
    th.genesis_mint_to_all(&Holder::Alice, &[100_000_000_000u64 * n as u64], 0).await?;

    let mut txs = Vec::with_capacity(n);
    for _ in 0..n {
        let token_blind = BaseBlind::random(&mut OsRng);
        let (tx, _mint_params, _auth_params, _fee_params) = th
            .token_mint(
                1_000_000_000u64,
                &Holder::Alice,
                &Holder::Alice,
                token_blind,
                None,
                None,
                block_height,
            )
            .await?;
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` DAO vote transactions.  Each votes on a distinct proposal with
/// a distinct gov token coin.  Uses `mark_spent_nullifier` between builds
/// because `dao_vote()` finds the first gov coin internally and doesn't
/// remove it from the wallet's unspent list.
async fn build_dao_vote_txs(
    th: &mut TestHarness,
    n: usize,
    dao_ctx: &DaoContext,
    block_height: u32,
    proposal_offset: usize,
) -> Result<Vec<Transaction>> {
    let gov_token_id = dao_ctx.gov_token_id;
    let mut txs = Vec::with_capacity(n);

    for i in 0..n {
        let gov_coins = th.coins_by_token(&Holder::Alice, gov_token_id);
        if gov_coins.is_empty() {
            warn!("Not enough gov token coins for {} vote txs (built {})", n, txs.len());
            break;
        }
        let used_nullifier = gov_coins[0].nullifier();

        let proposal = &dao_ctx.proposals[(i + proposal_offset).min(dao_ctx.proposals.len() - 1)].0;
        let (tx, _params, _fee_params) = th
            .dao_vote(&Holder::Alice, true, &dao_ctx.dao_obj, proposal, block_height)
            .await?;

        // Remove the used gov coin from the wallet's unspent list so the
        // next dao_vote() call finds a different coin.  We use retain()
        // instead of mark_spent_nullifier() to avoid inserting the nullifier
        // into the SMT (which would cause the validator's batch verification
        // to reject the tx as a double-spend).
        th.wallet_mut(&Holder::Alice).unspent_money_coins.retain(|c| c.nullifier() != used_nullifier);
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` DAO exec transactions.  Each executes a pre-voted proposal.
async fn build_dao_exec_txs(
    th: &mut TestHarness,
    n: usize,
    dao_ctx: &DaoContext,
    block_height: u32,
) -> Result<Vec<Transaction>> {
    let mut txs = Vec::with_capacity(n);
    for i in 0..n {
        let (proposal, coin_attrs) = &dao_ctx.proposals[i.min(dao_ctx.proposals.len() - 1)];
        let (yes_vote_value, all_vote_value, yes_blind, all_blind) =
            dao_ctx.vote_data[i.min(dao_ctx.vote_data.len() - 1)];

        // Find the first DAO treasury coin's nullifier before building,
        // so we can remove it after (dao_exec_transfer reads the Dao
        // wallet immutably and doesn't update it).
        let dao_coin_nullifier = {
            let dao_wallet = th.wallet(&Holder::Dao);
            dao_wallet
                .unspent_money_coins
                .iter()
                .find(|c| c.note.spend_hook != FuncId::none())
                .map(|c| c.nullifier())
        };

        let (tx, _params, _fee_params) = th
            .dao_exec_transfer(
                &Holder::Alice,
                &dao_ctx.dao_obj,
                &dao_ctx.dao_keys.exec.secret,
                &Some(dao_ctx.dao_keys.early_exec.secret),
                proposal,
                coin_attrs.clone(),
                yes_vote_value,
                all_vote_value,
                yes_blind,
                all_blind,
                block_height,
            )
            .await?;

        // Remove the spent treasury coin so the next exec tx selects a
        // different one (avoids DuplicateNullifier in batch verification).
        if let Some(nf) = dao_coin_nullifier {
            th.wallet_mut(&Holder::Dao).unspent_money_coins.retain(|c| c.nullifier() != nf);
        }
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` DAO propose transactions with `recipients_per_proposal`
/// recipients each.  Uses `mark_spent_nullifier` between builds because
/// `dao_propose_transfer()` finds the first gov coin internally.
async fn build_dao_propose_txs(
    th: &mut TestHarness,
    n: usize,
    recipients_per_proposal: usize,
    dao_ctx: &DaoContext,
    block_height: u32,
) -> Result<Vec<Transaction>> {
    let gov_token_id = dao_ctx.gov_token_id;
    let recipient_holders: Vec<Holder> =
        th.holder_keys.iter().filter(|h| **h != Holder::Dao).cloned().collect();

    let mut txs = Vec::with_capacity(n);
    for i in 0..n {
        let gov_coins = th.coins_by_token(&Holder::Alice, gov_token_id);
        if gov_coins.is_empty() {
            warn!("Not enough gov token coins for {} propose txs (built {})", n, txs.len());
            break;
        }
        let used_nullifier = gov_coins[0].nullifier();

        let coin_attrs: Vec<CoinAttributes> = (0..recipients_per_proposal)
            .map(|j| {
                let holder = recipient_holders[(i * recipients_per_proposal + j) % recipient_holders.len()];
                CoinAttributes {
                    public_key: th.wallet(&holder).keypair.public,
                    value: 1_000_000_000u64,
                    token_id: *DARK_TOKEN_ID,
                    spend_hook: FuncId::none(),
                    user_data: pallas::Base::ZERO,
                    blind: Blind::random(&mut OsRng),
                }
            })
            .collect();

        let (tx, _params, _fee_params, _proposal) = th
            .dao_propose_transfer(
                &Holder::Alice,
                &coin_attrs,
                pallas::Base::ZERO,
                &dao_ctx.dao_obj,
                &dao_ctx.dao_keys.proposer.secret,
                block_height,
                100,
            )
            .await?;

        // Remove the used gov coin from the wallet's unspent list so the
        // next dao_propose_transfer() call finds a different coin.
        th.wallet_mut(&Holder::Alice).unspent_money_coins.retain(|c| c.nullifier() != used_nullifier);
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` OTC swap transactions.  Each swaps a DARK coin (Alice) for a
/// DAWN coin (Bob).  Pre-mints `n` DARK coins and `n` DAWN coins so each
/// tx gets distinct inputs.
async fn build_otc_swap_txs(
    th: &mut TestHarness,
    n: usize,
    block_height: u32,
) -> Result<Vec<Transaction>> {
    // Mint N DARK coins to Alice
    th.genesis_mint_to_all(&Holder::Alice, &vec![10_000_000_000u64; n], 0).await?;

    // Mint N DAWN coins to Bob (same token, N separate mints)
    let dawn_blind = BaseBlind::random(&mut OsRng);
    let dawn_token_id = th.derive_token_id(&Holder::Bob, dawn_blind);
    for _ in 0..n {
        th.token_mint_with_blind_to_all(
            10_000_000_000u64,
            &Holder::Bob,
            &Holder::Bob,
            dawn_blind,
            block_height,
        )
        .await?;
    }

    let alice_coins = th.coins_by_token(&Holder::Alice, *DARK_TOKEN_ID);
    let bob_coins = th.coins_by_token(&Holder::Bob, dawn_token_id);

    let alice_swap_coins: Vec<_> = alice_coins.iter().filter(|c| c.note.value == 10_000_000_000).take(n).cloned().collect();
    let bob_swap_coins: Vec<_> = bob_coins.iter().take(n).cloned().collect();

    let mut txs = Vec::with_capacity(n);
    for i in 0..n.min(alice_swap_coins.len()).min(bob_swap_coins.len()) {
        let (tx, _params, _fee_params) = th
            .otc_swap(
                &Holder::Alice,
                &alice_swap_coins[i],
                &Holder::Bob,
                &bob_swap_coins[i],
                block_height,
            )
            .await?;
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` DAO mint transactions.  Each deploys a distinct DAO (different
/// `bulla_blind`).  No setup needed beyond `TestHarness::new()` (native
/// contracts are already deployed).
async fn build_dao_mint_txs(
    th: &mut TestHarness,
    n: usize,
    block_height: u32,
) -> Result<Vec<Transaction>> {
    let dao_keys = DaoKeys {
        notes: th.wallet(&Holder::Dao).keypair.clone(),
        proposer: Keypair::random(&mut OsRng),
        proposals: Keypair::random(&mut OsRng),
        votes: Keypair::random(&mut OsRng),
        exec: Keypair::random(&mut OsRng),
        early_exec: Keypair::random(&mut OsRng),
    };

    let mut txs = Vec::with_capacity(n);
    for _ in 0..n {
        let gov_token_blind = BaseBlind::random(&mut OsRng);
        let gov_token_id = th.derive_token_id(&Holder::Alice, gov_token_blind);

        let dao_obj = DaoModel {
            proposer_limit: 20_000_000_000,
            quorum: 10_000_000_000,
            early_exec_quorum: 10_000_000_000,
            approval_ratio_quot: 67,
            approval_ratio_base: 100,
            gov_token_id,
            notes_public_key: dao_keys.notes.public,
            proposer_public_key: dao_keys.proposer.public,
            proposals_public_key: dao_keys.proposals.public,
            votes_public_key: dao_keys.votes.public,
            exec_public_key: dao_keys.exec.public,
            early_exec_public_key: dao_keys.early_exec.public,
            bulla_blind: Blind::random(&mut OsRng),
        };

        let (tx, _params, _fee_params) = th
            .dao_mint(
                &Holder::Alice,
                &dao_obj,
                &dao_keys.notes.secret,
                &dao_keys.proposer.secret,
                &dao_keys.proposals.secret,
                &dao_keys.votes.secret,
                &dao_keys.exec.secret,
                &dao_keys.early_exec.secret,
                block_height,
            )
            .await?;
        txs.push(tx);
    }
    Ok(txs)
}

/// Build `n` deployment transactions using padded money contract WASM.
/// `target_kb` controls the padded WASM size.
async fn build_deploy_txs(
    th: &mut TestHarness,
    n: usize,
    block_height: u32,
    target_kb: usize,
) -> Result<Vec<Transaction>> {
    let base_wasm = std::fs::read(money_wasm_path())?;
    let padded_wasm = pad_wasm_to_size(&base_wasm, target_kb * 1024);

    th.genesis_mint_to_all(&Holder::Alice, &[100_000_000_000u64 * n as u64], 0).await?;

    let mut txs = Vec::with_capacity(n);
    for _ in 0..n {
        let (tx, _deploy_params, _fee_params) =
            th.deploy_contract(&Holder::Alice, padded_wasm.clone(), block_height).await?;
        txs.push(tx);
    }
    Ok(txs)
}

// ---------------------------------------------------------------------------
// Benchmark execution
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct TrialResult {
    tx_count: usize,
    verification_secs: f64,
    apply_secs: f64,
    total_secs: f64,
    gas_used: u64,
}

/// Verify (and optionally apply) a prebuilt tx batch with external
/// wall-clock timing.
///
/// `write=false` runs the verify phase only (the overlay is discarded, so
/// the prebuilt txs' coins are not spent and the batch can be re-run).
/// `write=true` additionally applies the state transitions.
///
/// `verify_fees` is always `false` because the txs are prebuilt without
/// fee calls (fee overhead is added separately via the `FEE_CALL_GAS`
/// constant).
///
/// On failure, logs each tx's function codes to aid debugging.
async fn verify_batch(
    th: &TestHarness,
    txs: &[Transaction],
    block_height: u32,
    write: bool,
    tx_labels: &[&str],
) -> Result<TrialResult> {
    let validator = th.wallet(&Holder::Alice).validator.read().await;
    let block_target = validator.consensus.module.target;

    debug!(
        "Verifying batch of {} txs at block_height={} (write={})",
        txs.len(),
        block_height,
        write
    );
    for (i, tx) in txs.iter().enumerate() {
        let label = tx_labels.get(i).copied().unwrap_or("?");
        let codes: Vec<u8> = tx.calls.iter().map(|c| c.data.data.first().copied().unwrap_or(0)).collect();
        debug!("  tx[{}] {}: {} calls, function_codes={:?}", i, label, tx.calls.len(), codes);
    }

    let start = Instant::now();
    let result = validator
        .add_test_transactions(txs, block_height, block_target, write, false)
        .await;
    let total_secs = start.elapsed().as_secs_f64();

    match result {
        Ok((gas_used, _paid)) => {
            Ok(TrialResult {
                tx_count: txs.len(),
                verification_secs: total_secs,
                apply_secs: 0.0,
                total_secs,
                gas_used,
            })
        }
        Err(e) => {
            // Log the full batch composition for debugging
            error!("Batch verification failed ({} txs): {}", txs.len(), e);
            for (i, tx) in txs.iter().enumerate() {
                let label = tx_labels.get(i).copied().unwrap_or("?");
                let codes: Vec<u8> =
                    tx.calls.iter().map(|c| c.data.data.first().copied().unwrap_or(0)).collect();
                error!("  tx[{}] {}: {} calls, function_codes={:?}", i, label, tx.calls.len(), codes);
            }
            Err(e)
        }
    }
}

/// Measure a scenario's capacity with repeated verify-only trials and a
/// single verify+apply pass.
///
/// Methodology:
///   1. 3 `write=false` runs, timed. The median `total_secs` is reported
///      as `verification_secs`; `gas_used` is taken from the median trial
///      (identical across trials since gas is computed during verify,
///      independent of `write`).
///   2. One `write=true` run (verify + apply). `apply_secs` is derived as
///      `max(0, apply_total − verification_secs)`, giving an honest
///      verify/apply split without an instrumented validator API.
///   3. `total_secs = verification_secs + apply_secs`.
///
/// No explicit warmup is performed: the tx-build phase (`build_*_txs` calls
/// the harness helpers that build and execute each tx) already primes the
/// ZK verifying-key cache and sled page cache before this function runs.
///
/// `write=false` for the repeated trials is essential: a `write=true` run
/// spends the prebuilt txs' coins (nullifiers recorded), so the next trial
/// would be rejected as a double-spend.
async fn measure_capacity(
    th: &TestHarness,
    all_txs: &[Transaction],
    block_height: u32,
    tx_labels: &[&str],
) -> Result<TrialResult> {
    if all_txs.is_empty() {
        return Ok(TrialResult::default());
    }

    let trials = TRIALS;

    // Measured verify-only trials.
    let mut trial_totals: Vec<f64> = Vec::with_capacity(trials);
    let mut last_gas: u64 = 0;
    for i in 0..trials.max(1) {
        let r = verify_batch(th, all_txs, block_height, false, tx_labels).await?;
        debug!("Trial {}/{}: total_secs={:.4}, gas={}", i + 1, trials.max(1), r.total_secs, r.gas_used);
        trial_totals.push(r.total_secs);
        last_gas = r.gas_used;
    }
    trial_totals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let verification_secs = trial_totals[trial_totals.len() / 2];

    // Verify + apply pass (single). Apply overhead is the difference between
    // this run's total and the median verify-only time.
    let apply_run = verify_batch(th, all_txs, block_height, true, tx_labels).await?;
    let apply_secs = (apply_run.total_secs - verification_secs).max(0.0);
    let total_secs = verification_secs + apply_secs;

    Ok(TrialResult {
        tx_count: all_txs.len(),
        verification_secs,
        apply_secs,
        total_secs,
        gas_used: last_gas,
    })
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CapacityOutput {
    machine_info: MachineInfo,
    fee_overhead_gas: u64,
    results: Vec<CapacityResults>,
}

#[derive(Serialize)]
struct CapacityResults {
    scenario: String,
    op_shape: OpShapeSer,
    tx_count: usize,
    gas_used: u64,
    gas_per_tx: f64,
    gas_per_tx_with_fee: f64,
    verification_secs: f64,
    apply_secs: f64,
    total_secs: f64,
    secs_per_tx: f64,
    tps: f64,
    fee_overhead_gas: u64,
    verify_fees: bool,
    block_gas_limit: u64,
}

#[derive(Serialize)]
struct OpShapeSer {
    op_kind: String,
    inputs: usize,
    outputs: usize,
    recipients: usize,
    deploy_kb: usize,
}

// ---------------------------------------------------------------------------
// Scenario runner
// ---------------------------------------------------------------------------

async fn run_scenario(
    scenario: &Scenario,
) -> Result<CapacityResults> {
    use Holder::{Alice, Bob, Dao};

    let block_height = 1u32;
    let mut th = TestHarness::new(&[Alice, Bob, Dao], false).await?;

    // Pre-populate state
    if scenario.prepopulate_coins > 0 {
        debug!("Pre-populating with {} filler coins", scenario.prepopulate_coins);
        pre_populate_merkle_tree(&mut th, &Alice, scenario.prepopulate_coins).await?;
    }

    // Setup DAO if needed.  setup_dao_context returns the final block
    // height after all setup txs have been executed; this must be used for
    // building and verifying the measured txs, otherwise DAO txs fail with
    // SnapshotTooOld (the merkle snapshot in the ZK proof is stale relative
    // to the verifying block height).
    let mut current_bh = block_height;
    let dao_ctx = match scenario.op_kind {
        OpKind::DaoPropose => {
            let setup = DaoSetup {
                num_gov_coins: scenario.batch_size,
                num_proposals: 0,
                num_pre_votes: 0,
                recipients_per_proposal: scenario.dao_recipients,
            };
            let (ctx, bh) = setup_dao_context(&mut th, &setup, current_bh).await?;
            current_bh = bh;
            Some(ctx)
        }
        OpKind::DaoVote => {
            let setup = DaoSetup {
                num_gov_coins: scenario.batch_size * 2,
                num_proposals: scenario.batch_size,
                num_pre_votes: 0,
                recipients_per_proposal: scenario.dao_recipients,
            };
            let (ctx, bh) = setup_dao_context(&mut th, &setup, current_bh).await?;
            current_bh = bh;
            Some(ctx)
        }
        OpKind::DaoExec => {
            let setup = DaoSetup {
                num_gov_coins: scenario.batch_size * 2,
                num_proposals: scenario.batch_size,
                num_pre_votes: scenario.batch_size,
                recipients_per_proposal: scenario.dao_recipients,
            };
            let (ctx, bh) = setup_dao_context(&mut th, &setup, current_bh).await?;
            current_bh = bh;
            Some(ctx)
        }
        OpKind::Mixed => {
            let mix = WorkloadMix::mixed();
            let num_proposals = mix.dao_execs + mix.dao_votes;
            let setup = DaoSetup {
                num_gov_coins: num_proposals * 2 + mix.dao_votes,
                num_proposals,
                num_pre_votes: mix.dao_execs,
                recipients_per_proposal: scenario.dao_recipients,
            };
            let (ctx, bh) = setup_dao_context(&mut th, &setup, current_bh).await?;
            current_bh = bh;
            Some(ctx)
        }
        _ => None,
    };

    // Build transactions (not timed).  Use current_bh so txs are built
    // against the state that setup left behind.
    debug!("Building transactions for {} at block_height={}", scenario.name, current_bh);
    let (all_txs, tx_labels): (Vec<Transaction>, Vec<&'static str>) = match scenario.op_kind {
        OpKind::Transfer => {
            let txs = build_transfer_txs(&mut th, scenario.batch_size, current_bh).await?;
            (txs, vec!["transfer"; scenario.batch_size])
        }
        OpKind::TransferNIn => {
            let txs = build_transfer_n_input_txs(&mut th, scenario.batch_size, scenario.transfer_inputs, current_bh).await?;
            (txs, vec!["transfer_n_in"; scenario.batch_size])
        }
        OpKind::TransferNOut => {
            let txs = build_transfer_n_output_txs(&mut th, scenario.batch_size, scenario.transfer_outputs, current_bh).await?;
            (txs, vec!["transfer_n_out"; scenario.batch_size])
        }
        OpKind::DaoPropose => {
            let ctx = dao_ctx.as_ref().unwrap();
            let txs = build_dao_propose_txs(&mut th, scenario.batch_size, scenario.dao_recipients, ctx, current_bh).await?;
            (txs, vec!["dao_propose"; scenario.batch_size])
        }
        OpKind::DaoExec => {
            let ctx = dao_ctx.as_ref().unwrap();
            let txs = build_dao_exec_txs(&mut th, scenario.batch_size, ctx, current_bh).await?;
            (txs, vec!["dao_exec"; scenario.batch_size])
        }
        OpKind::DaoVote => {
            let ctx = dao_ctx.as_ref().unwrap();
            let txs = build_dao_vote_txs(&mut th, scenario.batch_size, ctx, current_bh, 0).await?;
            (txs, vec!["dao_vote"; scenario.batch_size])
        }
        OpKind::OtcSwap => {
            let txs = build_otc_swap_txs(&mut th, scenario.batch_size, current_bh).await?;
            (txs, vec!["otc_swap"; scenario.batch_size])
        }
        OpKind::TokenMint => {
            let txs = build_token_mint_txs(&mut th, scenario.batch_size, current_bh).await?;
            (txs, vec!["token_mint"; scenario.batch_size])
        }
        OpKind::DaoMint => {
            let txs = build_dao_mint_txs(&mut th, scenario.batch_size, current_bh).await?;
            (txs, vec!["dao_mint"; scenario.batch_size])
        }
        OpKind::Deploy => {
            let txs = build_deploy_txs(&mut th, scenario.batch_size, current_bh, scenario.deploy_kb).await?;
            (txs, vec!["deploy"; scenario.batch_size])
        }
        OpKind::Mixed => {
            let mix = WorkloadMix::mixed();
            let ctx = dao_ctx.as_ref().unwrap();
            let txs = build_mixed_txs(&mut th, &mix, ctx, scenario.deploy_kb, current_bh, mix.dao_execs).await?;
            let labels = build_mixed_labels(&mix);
            (txs, labels)
        }
    };

    debug!("Total prebuilt txs: {}", all_txs.len());

    // Measure capacity
    let result = measure_capacity(&th, &all_txs, current_bh, &tx_labels).await?;

    let tps = if result.total_secs > 0.0 {
        result.tx_count as f64 / result.total_secs
    } else {
        0.0
    };
    let gas_per_tx =
        if result.tx_count > 0 { result.gas_used as f64 / result.tx_count as f64 } else { 0.0 };
    let gas_per_tx_with_fee = gas_per_tx + FEE_CALL_GAS as f64;
    let secs_per_tx = if result.tx_count > 0 {
        result.total_secs / result.tx_count as f64
    } else {
        0.0
    };

    Ok(CapacityResults {
        scenario: scenario.name.clone(),
        op_shape: OpShapeSer {
            op_kind: format!("{:?}", scenario.op_kind),
            inputs: scenario.transfer_inputs,
            outputs: scenario.transfer_outputs,
            recipients: scenario.dao_recipients,
            deploy_kb: scenario.deploy_kb,
        },
        tx_count: result.tx_count,
        gas_used: result.gas_used,
        gas_per_tx,
        gas_per_tx_with_fee,
        verification_secs: result.verification_secs,
        apply_secs: result.apply_secs,
        total_secs: result.total_secs,
        secs_per_tx,
        tps,
        fee_overhead_gas: FEE_CALL_GAS,
        verify_fees: false,
        block_gas_limit: BLOCK_GAS_LIMIT,
    })
}

/// Build a mixed workload (used by `mixed` scenario).
async fn build_mixed_txs(
    th: &mut TestHarness,
    mix: &WorkloadMix,
    dao_ctx: &DaoContext,
    deploy_kb: usize,
    block_height: u32,
    proposal_offset: usize,
) -> Result<Vec<Transaction>> {
    let mut all_txs = Vec::new();

    if mix.transfers > 0 {
        let txs = build_transfer_txs(th, mix.transfers, block_height).await?;
        debug!("  Built {} transfer txs", txs.len());
        all_txs.extend(txs);
    }
    if mix.token_mints > 0 {
        let txs = build_token_mint_txs(th, mix.token_mints, block_height).await?;
        debug!("  Built {} token_mint txs", txs.len());
        all_txs.extend(txs);
    }
    if mix.dao_votes > 0 {
        let txs = build_dao_vote_txs(th, mix.dao_votes, dao_ctx, block_height, proposal_offset).await?;
        debug!("  Built {} dao_vote txs", txs.len());
        all_txs.extend(txs);
    }
    if mix.dao_execs > 0 {
        let txs = build_dao_exec_txs(th, mix.dao_execs, dao_ctx, block_height).await?;
        debug!("  Built {} dao_exec txs", txs.len());
        all_txs.extend(txs);
    }
    if mix.deployments > 0 {
        let txs = build_deploy_txs(th, mix.deployments, block_height, deploy_kb).await?;
        debug!("  Built {} deploy txs ({}KB)", txs.len(), deploy_kb);
        all_txs.extend(txs);
    }

    Ok(all_txs)
}

/// Generate human-readable labels for each tx in a mixed workload batch,
/// matching the build order in `build_mixed_txs`.
fn build_mixed_labels(mix: &WorkloadMix) -> Vec<&'static str> {
    let mut labels = Vec::with_capacity(mix.total());
    labels.extend(std::iter::repeat("transfer").take(mix.transfers));
    labels.extend(std::iter::repeat("token_mint").take(mix.token_mints));
    labels.extend(std::iter::repeat("dao_vote").take(mix.dao_votes));
    labels.extend(std::iter::repeat("dao_exec").take(mix.dao_execs));
    labels.extend(std::iter::repeat("deploy").take(mix.deployments));
    labels
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    smol::block_on(async {
        // Use stderr for tracing so JSON output on stdout stays clean.
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
            .with_writer(std::io::stderr)
            .init();

        let machine_info = detect_machine_info();

        let scenarios = all_scenarios();

        let mut all_results = Vec::new();

        for scenario in &scenarios {
            info!("=== Scenario: {} ===", scenario.name);
            let result = run_scenario(scenario).await?;
            all_results.push(result);
        }

        let output = CapacityOutput {
            machine_info,
            fee_overhead_gas: FEE_CALL_GAS,
            results: all_results,
        };
        println!("{}", serde_json::to_string_pretty(&output).unwrap());

        Ok(())
    })
}
