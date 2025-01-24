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
use std::fmt;

use rand::rngs::OsRng;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::encode_base10,
    zk::{halo2::Field, proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses, Proof},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::{swap_v1::SwapCallBuilder, MoneyNote},
    model::{Coin, MoneyTransferParamsV1, TokenId},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        contract_id::MONEY_CONTRACT_ID, pedersen::pedersen_commitment_u64, poseidon_hash,
        BaseBlind, Blind, FuncId, PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::{
    async_trait, deserialize_async, AsyncEncodable, SerialDecodable, SerialEncodable,
};

use super::{money::BALANCE_BASE10_DECIMALS, Drk};

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
/// Half of the swap data, includes the coin that is supposed to be sent,
/// and the coin that is supposed to be received.
pub struct PartialSwapData {
    params: MoneyTransferParamsV1,
    proofs: Vec<Proof>,
    value_pair: (u64, u64),
    token_pair: (TokenId, TokenId),
    value_blinds: Vec<ScalarBlind>,
    token_blinds: Vec<BaseBlind>,
}

impl fmt::Display for PartialSwapData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s =
            format!(
            "{:#?}\nValue pair: {}:{}\nToken pair: {}:{}\nValue blinds: {:?}\nToken blinds: {:?}\n",
            self.params, self.value_pair.0, self.value_pair.1, self.token_pair.0, self.token_pair.1,
            self.value_blinds, self.token_blinds,
        );

        write!(f, "{}", s)
    }
}

impl Drk {
    /// Initialize the first half of an atomic swap
    pub async fn init_swap(
        &self,
        value_pair: (u64, u64),
        token_pair: (TokenId, TokenId),
        user_data_blind_send: Option<BaseBlind>,
        spend_hook_recv: Option<FuncId>,
        user_data_recv: Option<pallas::Base>,
    ) -> Result<PartialSwapData> {
        // First get all unspent OwnCoins to see what our balance is
        let owncoins = self.get_token_coins(&token_pair.0).await?;
        if owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "Did not find any unspent coins with token ID: {}",
                token_pair.0
            )))
        }

        // Find one with the correct value
        let mut burn_coin = None;
        for coin in owncoins {
            if coin.note.value == value_pair.0 {
                burn_coin = Some(coin);
                break
            }
        }
        let Some(burn_coin) = burn_coin else {
            return Err(Error::Custom(format!(
                "Did not find any unspent coins of value {} and token_id {}",
                value_pair.0, token_pair.0,
            )))
        };

        // Fetch our default address
        let address = self.default_address().await?;

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

        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin.1)?;

        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin)?, &mint_zkbin);
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin)?, &burn_zkbin);

        // Creating Mint and Burn circuits proving keys
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);

        // Since we're creating the first half, we generate the blinds.
        let value_blinds = [Blind::random(&mut OsRng), Blind::random(&mut OsRng)];
        let token_blinds = [Blind::random(&mut OsRng), Blind::random(&mut OsRng)];

        // Now we should have everything we need to build the swap half
        let builder = SwapCallBuilder {
            pubkey: address,
            value_send: value_pair.0,
            token_id_send: token_pair.0,
            value_recv: value_pair.1,
            token_id_recv: token_pair.1,
            user_data_blind_send: user_data_blind_send.unwrap_or(Blind::random(&mut OsRng)),
            spend_hook_recv: spend_hook_recv.unwrap_or(FuncId::none()),
            user_data_recv: user_data_recv.unwrap_or(pallas::Base::ZERO),
            value_blinds,
            token_blinds,
            coin: burn_coin,
            tree,
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
        };
        let debris = builder.build()?;

        // Now we have the half, so we can build `PartialSwapData` and return it.
        let ret = PartialSwapData {
            params: debris.params,
            proofs: debris.proofs,
            value_pair,
            token_pair,
            value_blinds: value_blinds.to_vec(),
            token_blinds: token_blinds.to_vec(),
        };

        Ok(ret)
    }

    /// Create a full transaction by inspecting and verifying given partial swap data,
    /// making the other half, and joining all this into a `Transaction` object.
    pub async fn join_swap(
        &self,
        partial: PartialSwapData,
        user_data_blind_send: Option<BaseBlind>,
        spend_hook_recv: Option<FuncId>,
        user_data_recv: Option<pallas::Base>,
    ) -> Result<Transaction> {
        // Our side of the tx in the pairs is the second half, so we try to find
        // an unspent coin like that in our wallet.
        let owncoins = self.get_token_coins(&partial.token_pair.1).await?;
        if owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "Did not find any unspent coins with token ID: {}",
                partial.token_pair.1
            )))
        }

        // Find one with the correct value
        let mut burn_coin = None;
        for coin in owncoins {
            if coin.note.value == partial.value_pair.1 {
                burn_coin = Some(coin);
                break
            }
        }
        let Some(burn_coin) = burn_coin else {
            return Err(Error::Custom(format!(
                "Did not find any unspent coins of value {} and token_id {}",
                partial.value_pair.1, partial.token_pair.1,
            )))
        };

        // Fetch our default address
        let address = self.default_address().await?;

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

        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin.1)?;

        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin)?, &mint_zkbin);
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin)?, &burn_zkbin);

        // Creating Mint and Burn circuits proving keys
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);

        // Now we should have everything we need to build the swap half
        let builder = SwapCallBuilder {
            pubkey: address,
            value_send: partial.value_pair.1,
            token_id_send: partial.token_pair.1,
            value_recv: partial.value_pair.0,
            token_id_recv: partial.token_pair.0,
            user_data_blind_send: user_data_blind_send.unwrap_or(Blind::random(&mut OsRng)),
            spend_hook_recv: spend_hook_recv.unwrap_or(FuncId::none()),
            user_data_recv: user_data_recv.unwrap_or(pallas::Base::ZERO),
            value_blinds: [partial.value_blinds[1], partial.value_blinds[0]],
            token_blinds: [partial.token_blinds[1], partial.token_blinds[0]],
            coin: burn_coin,
            tree,
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
        };
        let debris = builder.build()?;

        // Build the full transaction
        let full_params = MoneyTransferParamsV1 {
            inputs: vec![partial.params.inputs[0].clone(), debris.params.inputs[0].clone()],
            outputs: vec![partial.params.outputs[0].clone(), debris.params.outputs[0].clone()],
        };

        let full_proofs = vec![
            partial.proofs[0].clone(),
            debris.proofs[0].clone(),
            partial.proofs[1].clone(),
            debris.proofs[1].clone(),
        ];

        let mut data = vec![MoneyFunction::OtcSwapV1 as u8];
        full_params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: full_proofs }, vec![])?;
        let mut tx = tx_builder.build()?;

        // Sign the transaction and return it
        let sigs = tx.create_sigs(&[debris.signature_secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Inspect and verify a given swap (half or full) transaction
    pub async fn inspect_swap(&self, bytes: Vec<u8>) -> Result<()> {
        // First we check if its a partial swap
        if let Ok(partial) = deserialize_async::<PartialSwapData>(&bytes).await {
            // Inspect the PartialSwapData
            println!("{partial}");
            return Ok(())
        }

        // Try to deserialize a full swap transaction
        let Ok(tx) = deserialize_async::<Transaction>(&bytes).await else {
            return Err(Error::Custom(
                "Failed to deserialize to Transaction or PartialSwapData".to_string(),
            ))
        };

        // Default error to return in case insection fails
        let insection_error = Err(Error::Custom("Inspection failed".to_string()));

        // We're inspecting a full transaction
        if tx.calls.len() != 1 {
            eprintln!(
                "Found {} contract calls in the transaction, there should be 1",
                tx.calls.len()
            );
            return insection_error
        }

        let params: MoneyTransferParamsV1 = deserialize_async(&tx.calls[0].data.data[1..]).await?;
        println!("Parameters:\n{:#?}", params);

        if params.inputs.len() != 2 {
            eprintln!("Found {} inputs, there should be 2", params.inputs.len());
            return insection_error
        }

        if params.outputs.len() != 2 {
            eprintln!("Found {} outputs, there should be 2", params.outputs.len());
            return insection_error
        }

        // Try to decrypt one of the outputs.
        let secret_keys = self.get_money_secrets().await?;
        let mut skey: Option<SecretKey> = None;
        let mut note: Option<MoneyNote> = None;
        let mut output_idx = 0;

        for output in &params.outputs {
            println!("Trying to decrypt note in output {output_idx}");

            for secret in &secret_keys {
                if let Ok(d_note) = output.note.decrypt::<MoneyNote>(secret) {
                    let s: SecretKey = deserialize_async(&d_note.memo).await?;
                    skey = Some(s);
                    note = Some(d_note);
                    println!("Successfully decrypted and found an ephemeral secret");
                    break
                }
            }

            if note.is_some() {
                break
            }

            output_idx += 1;
        }

        let Some(note) = note else {
            eprintln!("Error: Could not decrypt notes of either output");
            return insection_error
        };

        println!(
            "Output[{output_idx}] value: {} ({})",
            note.value,
            encode_base10(note.value, BALANCE_BASE10_DECIMALS)
        );
        println!("Output[{output_idx}] token ID: {}", note.token_id);

        let skey = skey.unwrap();
        let (pub_x, pub_y) = PublicKey::from_secret(skey).xy();
        let coin = Coin::from(poseidon_hash([
            pub_x,
            pub_y,
            pallas::Base::from(note.value),
            note.token_id.inner(),
            note.coin_blind.inner(),
        ]));

        if coin == params.outputs[output_idx].coin {
            println!("Output[{output_idx}] coin matches decrypted note metadata");
        } else {
            eprintln!("Error: Output[{output_idx}] coin does not match note metadata");
            return insection_error
        }

        let valcom = pedersen_commitment_u64(note.value, note.value_blind);
        let tokcom = poseidon_hash([note.token_id.inner(), note.token_blind.inner()]);

        if valcom != params.outputs[output_idx].value_commit {
            eprintln!("Error: Output[{output_idx}] value commitment does not match note metadata");
            return insection_error
        }

        if tokcom != params.outputs[output_idx].token_commit {
            eprintln!("Error: Output[{output_idx}] token commitment does not match note metadata");
            return insection_error
        }

        println!("Value and token commitments match decrypted note metadata");

        // Verify that the output commitments match the other input commitments
        match output_idx {
            0 => {
                if valcom != params.inputs[1].value_commit ||
                    tokcom != params.inputs[1].token_commit
                {
                    eprintln!("Error: Value/Token commits of output[0] do not match input[1]");
                    return insection_error
                }
            }
            1 => {
                if valcom != params.inputs[0].value_commit ||
                    tokcom != params.inputs[0].token_commit
                {
                    eprintln!("Error: Value/Token commits of output[1] do not match input[0]");
                    return insection_error
                }
            }
            _ => unreachable!(),
        }

        println!("Found matching pedersen commitments for outputs and inputs");

        Ok(())
    }

    /// Sign given swap transaction by retrieving the secret key from the encrypted
    /// note and prepending it to the transaction's signatures.
    pub async fn sign_swap(&self, tx: &mut Transaction) -> Result<()> {
        // We need our secret keys to try and decrypt the notes
        let secret_keys = self.get_money_secrets().await?;
        let params: MoneyTransferParamsV1 = deserialize_async(&tx.calls[0].data.data[1..]).await?;

        // We wil try to decrypt each note separately,
        // since we might us the same key in both of them.
        let mut found = false;

        // Try to decrypt the first note
        for secret in &secret_keys {
            let Ok(note) = &params.outputs[0].note.decrypt::<MoneyNote>(secret) else { continue };

            // Sign the swap transaction
            let skey: SecretKey = deserialize_async(&note.memo).await?;
            let sigs = tx.create_sigs(&[skey])?;

            // If transaction contains both signatures, replace the first one,
            // otherwise insert signature on first position.
            if tx.signatures[0].len() == 2 {
                tx.signatures[0][0] = sigs[0];
            } else {
                tx.signatures[0].insert(0, sigs[0]);
            }

            found = true;
            break
        }

        // Try to decrypt the second note
        for secret in &secret_keys {
            let Ok(note) = &params.outputs[1].note.decrypt::<MoneyNote>(secret) else { continue };

            // Sign the swap transaction
            let skey: SecretKey = deserialize_async(&note.memo).await?;
            let sigs = tx.create_sigs(&[skey])?;

            // If transaction contains both signatures, replace the second one,
            // otherwise replace the first one.
            if tx.signatures[0].len() == 2 {
                tx.signatures[0][1] = sigs[0];
            } else {
                tx.signatures[0][0] = sigs[0];
            }

            found = true;
            break
        }

        if !found {
            eprintln!("Error: Failed to decrypt note with any of our secret keys");
            return Err(Error::Custom(
                "Failed to decrypt note with any of our secret keys".to_string(),
            ))
        };

        Ok(())
    }
}
