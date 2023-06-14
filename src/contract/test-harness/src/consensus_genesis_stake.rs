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
use darkfi_consensus_contract::{
    client::genesis_stake_v1::ConsensusGenesisStakeCallBuilder,
    model::ConsensusGenesisStakeParamsV1, ConsensusFunction,
};
use darkfi_money_contract::CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1;
use darkfi_sdk::{
    crypto::{MerkleNode, CONSENSUS_CONTRACT_ID},
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn genesis_stake(
        &mut self,
        holder: Holder,
        amount: u64,
    ) -> Result<(Transaction, ConsensusGenesisStakeParamsV1)> {
        let wallet = self.holders.get(&holder).unwrap();
        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusGenesisStake).unwrap();
        let timer = Instant::now();

        // Building Consensus::GenesisStake params
        let genesis_stake_call_debris = ConsensusGenesisStakeCallBuilder {
            keypair: wallet.keypair,
            recipient: wallet.keypair.public,
            amount,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        }
        .build()?;
        let (genesis_stake_params, genesis_stake_proofs) =
            (genesis_stake_call_debris.params, genesis_stake_call_debris.proofs);

        // Building genesis stake tx
        let mut data = vec![ConsensusFunction::GenesisStakeV1 as u8];
        genesis_stake_params.encode(&mut data)?;
        let contract_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };
        let calls = vec![contract_call];
        let proofs = vec![genesis_stake_proofs];
        let mut genesis_stake_tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = genesis_stake_tx.create_sigs(&mut OsRng, &[wallet.keypair.secret])?;
        genesis_stake_tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&genesis_stake_tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((genesis_stake_tx, genesis_stake_params))
    }

    pub async fn execute_genesis_stake_tx(
        &mut self,
        holder: Holder,
        tx: &Transaction,
        params: &ConsensusGenesisStakeParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusGenesisStake).unwrap();
        let timer = Instant::now();

        let erroneous_txs =
            wallet.state.read().await.verify_transactions(&[tx.clone()], slot, true).await?;
        assert!(erroneous_txs.is_empty());
        wallet.consensus_staked_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub async fn execute_erroneous_genesis_stake_txs(
        &mut self,
        holder: Holder,
        txs: Vec<Transaction>,
        slot: u64,
        erroneous: usize,
    ) -> Result<()> {
        let wallet = self.holders.get(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusGenesisStake).unwrap();
        let timer = Instant::now();

        let erroneous_txs =
            wallet.state.read().await.verify_transactions(&txs, slot, false).await?;
        assert_eq!(erroneous_txs.len(), erroneous);
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
