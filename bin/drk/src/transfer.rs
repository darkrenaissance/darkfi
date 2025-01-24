/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    client::transfer_v1::make_transfer_call, model::TokenId, MoneyFunction,
    MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_FEE_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, FuncId, Keypair, PublicKey},
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::AsyncEncodable;

use crate::{money::BALANCE_BASE10_DECIMALS, Drk};

impl Drk {
    /// Create a payment transaction. Returns the transaction object on success.
    pub async fn transfer(
        &self,
        amount: &str,
        token_id: TokenId,
        recipient: PublicKey,
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
        half_split: bool,
    ) -> Result<Transaction> {
        // First get all unspent OwnCoins to see what our balance is
        let owncoins = self.get_token_coins(&token_id).await?;
        if owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "Did not find any unspent coins with token ID: {token_id}"
            )))
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

        // Fetch our default secret
        let secret = self.default_secret().await?;
        let keypair = Keypair::new(secret);

        // We'll also need our Merkle tree
        let tree = self.get_money_tree().await?;

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(mint_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_MINT_NS_V1)
        else {
            return Err(Error::Custom("Mint circuit not found".to_string()))
        };

        let Some(burn_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_BURN_NS_V1)
        else {
            return Err(Error::Custom("Burn circuit not found".to_string()))
        };

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("Fee circuit not found".to_string()))
        };

        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin.1)?;
        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin)?, &mint_zkbin);
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin)?, &burn_zkbin);
        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Mint, Burn and Fee circuits proving keys
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Building transaction parameters
        let (params, secrets, spent_coins) = make_transfer_call(
            keypair,
            recipient,
            amount,
            token_id,
            owncoins,
            tree.clone(),
            spend_hook,
            user_data,
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
            half_split,
        )?;

        // Encode the call
        let mut data = vec![MoneyFunction::TransferV1 as u8];
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the TransactionBuilder containing the `Transfer` call
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: secrets.proofs }, vec![])?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        // We also tell it about any spent coins so we don't accidentally reuse them in the
        // fee call.
        // TODO: We have to build a proper coin selection algorithm so that we can utilize
        // the Money::Transfer to merge any coins which would give us a coin with enough
        // value for paying the transaction fee.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&secrets.signature_secrets)?;
        tx.signatures.push(sigs);

        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, Some(&spent_coins)).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&secrets.signature_secrets)?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }
}
