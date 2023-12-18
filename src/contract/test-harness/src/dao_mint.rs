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
use darkfi_dao_contract::{
    client, model::{Dao, DaoMintParams}, DaoFunction, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
};
use darkfi_sdk::{
    crypto::{Keypair, MerkleNode, DAO_CONTRACT_ID},
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn dao_mint(
        &mut self,
        dao_info: &Dao,
        dao_kp: &Keypair,
    ) -> Result<(Transaction, DaoMintParams)> {
        let (dao_mint_pk, dao_mint_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_MINT_NS.to_string()).unwrap();

        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::DaoMint).unwrap();
        let timer = Instant::now();

        let (params, proofs) =
            client::make_mint_call(dao_info, &dao_kp.secret, dao_mint_zkbin, dao_mint_pk)?;

        let mut data = vec![DaoFunction::Mint as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *DAO_CONTRACT_ID, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &[dao_kp.secret])?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, params))
    }

    pub async fn execute_dao_mint_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &DaoMintParams,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::DaoMint).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], slot, true).await?;
        wallet.dao_merkle_tree.append(MerkleNode::from(params.dao_bulla.inner()));
        let leaf_pos = wallet.dao_merkle_tree.mark().unwrap();
        wallet.dao_leafs.insert(params.dao_bulla, leaf_pos);

        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
