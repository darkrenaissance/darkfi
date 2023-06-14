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

use std::time::Instant;

use darkfi::{tx::Transaction, Result};
use darkfi_consensus_contract::{client::stake_v1::ConsensusStakeCallBuilder, ConsensusFunction};
use darkfi_money_contract::{
    client::{stake_v1::MoneyStakeCallBuilder, OwnCoin},
    model::ConsensusStakeParamsV1,
    MoneyFunction, CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1, MONEY_CONTRACT_ZKAS_BURN_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, SecretKey, CONSENSUS_CONTRACT_ID, MONEY_CONTRACT_ID},
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub async fn stake(
        &mut self,
        holder: Holder,
        slot: u64,
        owncoin: OwnCoin,
    ) -> Result<(Transaction, ConsensusStakeParamsV1, SecretKey)> {
        let wallet = self.holders.get(&holder).unwrap();
        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let (burn_pk, burn_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusStake).unwrap();
        let epoch = wallet.state.read().await.consensus.time_keeper.slot_epoch(slot);
        let timer = Instant::now();

        // Building Money::Stake params
        let money_stake_call_debris = MoneyStakeCallBuilder {
            coin: owncoin.clone(),
            tree: wallet.money_merkle_tree.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        }
        .build()?;

        let (
            money_stake_params,
            money_stake_proofs,
            money_stake_secret_key,
            money_stake_value_blind,
        ) = (
            money_stake_call_debris.params,
            money_stake_call_debris.proofs,
            money_stake_call_debris.signature_secret,
            money_stake_call_debris.value_blind,
        );

        // Building Consensus::Stake params
        let consensus_stake_call_debris = ConsensusStakeCallBuilder {
            coin: owncoin,
            epoch,
            value_blind: money_stake_value_blind,
            money_input: money_stake_params.input.clone(),
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        }
        .build()?;

        let (consensus_stake_params, consensus_stake_proofs, consensus_stake_secret_key) = (
            consensus_stake_call_debris.params,
            consensus_stake_call_debris.proofs,
            consensus_stake_call_debris.signature_secret,
        );

        // Building stake tx
        let mut data = vec![MoneyFunction::StakeV1 as u8];
        money_stake_params.encode(&mut data)?;
        let money_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let mut data = vec![ConsensusFunction::StakeV1 as u8];
        consensus_stake_params.encode(&mut data)?;
        let consensus_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };

        let calls = vec![money_call, consensus_call];
        let proofs = vec![money_stake_proofs, consensus_stake_proofs];
        let mut stake_tx = Transaction { calls, proofs, signatures: vec![] };
        let money_sigs = stake_tx.create_sigs(&mut OsRng, &[money_stake_secret_key])?;
        let consensus_sigs = stake_tx.create_sigs(&mut OsRng, &[consensus_stake_secret_key])?;
        stake_tx.signatures = vec![money_sigs, consensus_sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&stake_tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((stake_tx, consensus_stake_params, consensus_stake_secret_key))
    }

    pub async fn execute_stake_tx(
        &mut self,
        holder: Holder,
        tx: &Transaction,
        params: &ConsensusStakeParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusStake).unwrap();
        let timer = Instant::now();

        let erroneous_txs =
            wallet.state.read().await.verify_transactions(&[tx.clone()], slot, true).await?;
        assert!(erroneous_txs.is_empty());
        wallet.consensus_staked_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
