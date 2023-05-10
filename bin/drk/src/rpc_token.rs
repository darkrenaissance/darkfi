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
    util::parse::decode_base10,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
};
use darkfi_money_contract::{
    client::{freeze_v1::FreezeCallBuilder, mint_v1::MintCallBuilder},
    MoneyFunction, MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, Keypair, PublicKey, TokenId},
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;

use super::Drk;

impl Drk {
    /// Create a token mint transaction. Returns the transaction object on success.
    pub async fn mint_token(
        &self,
        amount: &str,
        recipient: PublicKey,
        token_id: TokenId,
    ) -> Result<Transaction> {
        // TODO: Mint directly into DAO treasury
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();

        let amount = decode_base10(amount, 8, false)?;

        let mut tokens = self.list_tokens().await?;
        tokens.retain(|x| x.0 == token_id);
        if tokens.is_empty() {
            return Err(anyhow!("Did not find mint authority for token ID {}", token_id))
        }
        assert!(tokens.len() == 1);

        let mint_authority = Keypair::new(tokens[0].1);

        if tokens[0].2 {
            return Err(anyhow!("This token mint is marked as frozen in the wallet"))
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;
        let zkas_ns = MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1;

        let Some(token_mint_zkbin) = zkas_bins.iter().find(|x| x.0 == zkas_ns) else {
            return Err(anyhow!("Token mint circuit not found"))
        };

        let k = 13;
        let token_mint_zkbin = ZkBinary::decode(&token_mint_zkbin.1)?;
        let token_mint_circuit =
            ZkCircuit::new(empty_witnesses(&token_mint_zkbin), token_mint_zkbin.clone());

        eprintln!("Creating token mint circuit proving keys");
        let mint_builder = MintCallBuilder {
            mint_authority,
            recipient,
            amount,
            spend_hook,
            user_data,
            token_mint_zkbin,
            token_mint_pk: ProvingKey::build(k, &token_mint_circuit),
        };

        eprintln!("Building transaction parameters");
        let debris = mint_builder.build()?;

        // Encode and sign the transaction
        let mut data = vec![MoneyFunction::MintV1 as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &[mint_authority.secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Create a token freeze transaction. Returns the transaction object on success.
    pub async fn freeze_token(&self, token_id: TokenId) -> Result<Transaction> {
        let mut tokens = self.list_tokens().await?;
        tokens.retain(|x| x.0 == token_id);
        if tokens.is_empty() {
            return Err(anyhow!("Did not find mint authority for token ID {}", token_id))
        }
        assert!(tokens.len() == 1);

        let mint_authority = Keypair::new(tokens[0].1);

        if tokens[0].2 {
            return Err(anyhow!("This token is already marked as frozen in the wallet"))
        }

        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;
        let zkas_ns = MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1;

        let Some(token_freeze_zkbin) = zkas_bins.iter().find(|x| x.0 == zkas_ns) else {
            return Err(anyhow!("Token freeze circuit not found"))
        };

        let k = 13;
        let token_freeze_zkbin = ZkBinary::decode(&token_freeze_zkbin.1)?;
        let token_freeze_circuit =
            ZkCircuit::new(empty_witnesses(&token_freeze_zkbin), token_freeze_zkbin.clone());

        eprintln!("Creating token freeze circuit proving keys");
        let freeze_builder = FreezeCallBuilder {
            mint_authority,
            token_freeze_zkbin,
            token_freeze_pk: ProvingKey::build(k, &token_freeze_circuit),
        };

        eprintln!("Building transaction parameters");
        let debris = freeze_builder.build()?;

        // Encode and sign the transaction
        let mut data = vec![MoneyFunction::FreezeV1 as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &[mint_authority.secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }
}
