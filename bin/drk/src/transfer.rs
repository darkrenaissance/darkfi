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

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::{decode_base10, encode_base10},
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::{transfer_v1::make_transfer_call, OwnCoin},
    model::TokenId,
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, FuncId, Keypair, PublicKey},
    tx::ContractCall,
};
use darkfi_serial::Encodable;

use crate::{money::BALANCE_BASE10_DECIMALS, Drk};

impl Drk {
    /// Create a payment transaction. Returns the transaction object on success.
    pub async fn transfer(
        &self,
        amount: &str,
        token_id: TokenId,
        recipient: PublicKey,
    ) -> Result<Transaction> {
        // First get all unspent OwnCoins to see what our balance is.
        eprintln!("Fetching OwnCoins");
        let owncoins = self.get_coins(false).await?;
        let mut owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        // We're only interested in the ones for the token_id we're sending
        // And the ones not owned by some protocol (meaning spend-hook should be 0)
        owncoins.retain(|x| x.note.token_id == token_id);
        owncoins.retain(|x| x.note.spend_hook == FuncId::none());
        if owncoins.is_empty() {
            return Err(Error::Custom(format!("Did not find any coins with token ID: {token_id}")))
        }

        let amount = decode_base10(amount, BALANCE_BASE10_DECIMALS, false)?;
        let mut balance = 0;
        for coin in owncoins.iter() {
            balance += coin.note.value;
        }

        if balance < amount {
            return Err(Error::Custom(format!(
                "Not enough balance for token ID: {token_id}, found: {}",
                encode_base10(balance, BALANCE_BASE10_DECIMALS)
            )))
        }

        // We'll also need our Merkle tree
        let tree = self.get_money_tree().await?;

        let secret = self.default_secret().await?;
        let keypair = Keypair::new(secret);

        let contract_id = *MONEY_CONTRACT_ID;

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&contract_id).await?;

        let Some(mint_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_MINT_NS_V1)
        else {
            return Err(Error::Custom("Mint circuit not found".to_string()))
        };

        let Some(burn_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_BURN_NS_V1)
        else {
            return Err(Error::Custom("Burn circuit not found".to_string()))
        };

        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin.1)?;

        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin)?, &mint_zkbin);
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin)?, &burn_zkbin);

        eprintln!("Creating Mint and Burn circuit proving keys");
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);

        eprintln!("Building transaction parameters");
        let (params, secrets, spent_coins) = make_transfer_call(
            keypair, recipient, amount, token_id, owncoins, tree, mint_zkbin, mint_pk, burn_zkbin,
            burn_pk,
        )?;

        // Encode and sign the transaction
        let mut data = vec![MoneyFunction::TransferV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: secrets.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&secrets.signature_secrets)?;
        tx.signatures = vec![sigs];

        // We need to mark the coins we've spent in our wallet
        for spent_coin in spent_coins {
            if let Err(e) = self.mark_spent_coin(&spent_coin.coin).await {
                return Err(Error::Custom(format!("Mark spent coin {spent_coin:?} failed: {e:?}")))
            };
        }

        Ok(tx)
    }
}
