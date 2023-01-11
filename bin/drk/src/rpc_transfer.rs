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

use anyhow::{anyhow, Result};
use darkfi::{
    tx::Transaction,
    util::parse::{decode_base10, encode_base10},
    zk::{halo2::Field, proof::ProvingKey, vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
};
use darkfi_dao_contract::dao_model::DaoBulla;
use darkfi_money_contract::{
    client::{build_transfer_tx, OwnCoin},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        Keypair, PublicKey, TokenId,
    },
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;

use super::Drk;

impl Drk {
    /// Create a payment transaction. Returns the transaction object on success.
    pub async fn transfer(
        &self,
        amount: &str,
        token_id: TokenId,
        recipient: PublicKey,
        dao: bool,
        dao_bulla: Option<String>,
    ) -> Result<Transaction> {
        let dao_bulla: Option<DaoBulla> = if dao {
            let Some(dao_bulla) = dao_bulla else {
                return Err(anyhow!("Missing DAO bulla in parameters"))
            };

            Some(DaoBulla::try_from(dao_bulla.as_str())?)
        } else {
            None
        };

        let (spend_hook, user_data, user_data_blind) = if dao {
            (DAO_CONTRACT_ID.inner(), dao_bulla.unwrap().inner(), pallas::Base::random(&mut OsRng))
        } else {
            (pallas::Base::zero(), pallas::Base::zero(), pallas::Base::random(&mut OsRng))
        };

        // First get all unspent OwnCoins to see what our balance is.
        eprintln!("Fetching OwnCoins");
        let owncoins = self.wallet_coins(false).await?;
        let mut owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        // We're only interested in the ones for the token_id we're sending
        // And the ones not owned by some protocol (meaning spend-hook should be 0)
        owncoins.retain(|x| x.note.token_id == token_id);
        owncoins.retain(|x| x.note.spend_hook == pallas::Base::zero());
        if owncoins.is_empty() {
            return Err(anyhow!("Did not find any coins with token ID: {}", token_id))
        }

        // FIXME: Do not hardcode 8 decimals
        let amount = decode_base10(amount, 8, false)?;
        let mut balance = 0;
        for coin in owncoins.iter() {
            balance += coin.note.value;
        }

        if balance < amount {
            return Err(anyhow!(
                "Not enough balance for token ID: {}, found: {}",
                token_id,
                encode_base10(balance, 8)
            ))
        }

        // We'll also need our Merkle tree
        let tree = self.wallet_tree().await?;

        // TODO: Which keypair to actually use?
        let secrets = self.wallet_secrets().await?;
        let keypair = Keypair::new(secrets[0]);

        let contract_id = *MONEY_CONTRACT_ID;

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&contract_id).await?;

        let Some(mint_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_MINT_NS_V1) else {
            return Err(anyhow!("Mint circuit not found"))
        };

        let Some(burn_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_BURN_NS_V1) else {
            return Err(anyhow!("Burn circuit not found"))
        };

        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin.1)?;

        let k = 13;
        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin), mint_zkbin.clone());
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin), burn_zkbin.clone());

        eprintln!("Creating Mint circuit proving key");
        let mint_pk = ProvingKey::build(k, &mint_circuit);
        eprintln!("Creating Burn circuit proving key");
        let burn_pk = ProvingKey::build(k, &burn_circuit);

        // Now we should have everything we need to build the transaction
        let (params, proofs, secrets, spent_coins) = build_transfer_tx(
            &keypair,
            &recipient,
            amount,
            token_id,
            spend_hook,
            user_data,
            user_data_blind,
            &owncoins,
            &tree,
            &mint_zkbin,
            &mint_pk,
            &burn_zkbin,
            &burn_pk,
            false,
        )?;

        // Encode and sign the transaction
        let mut data = vec![MoneyFunction::Transfer as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &secrets)?;
        tx.signatures = vec![sigs];

        // We need to mark the coins we've spent in our wallet
        for spent_coin in spent_coins {
            self.mark_spent_coin(&spent_coin.coin).await?;
        }

        Ok(tx)
    }
}
