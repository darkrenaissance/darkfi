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
    Result,
};
use darkfi_dao_contract::{
    client::{DaoAuthMoneyTransferCall, DaoExecCall},
    model::{Dao, DaoBulla, DaoExecParams, DaoProposal},
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_ENC_COIN_NS,
    DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_NS, DAO_CONTRACT_ZKAS_DAO_EXEC_NS,
};
use darkfi_money_contract::{
    client::{transfer_v1 as xfer, MoneyNote, OwnCoin},
    model::{CoinAttributes, MoneyFeeParamsV1, MoneyTransferParamsV1},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        pedersen_commitment_u64, Blind, FuncRef, MerkleNode, ScalarBlind, SecretKey,
    },
    dark_tree::DarkTree,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use log::debug;
use rand::rngs::OsRng;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Dao::Exec` transaction.
    #[allow(clippy::too_many_arguments)]
    pub async fn dao_exec(
        &mut self,
        holder: &Holder,
        dao: &Dao,
        dao_bulla: &DaoBulla,
        proposal: &DaoProposal,
        proposal_coinattrs: Vec<CoinAttributes>,
        yes_vote_value: u64,
        all_vote_value: u64,
        yes_vote_blind: ScalarBlind,
        all_vote_blind: ScalarBlind,
        block_height: u64,
    ) -> Result<(Transaction, MoneyTransferParamsV1, DaoExecParams, Option<MoneyFeeParamsV1>)> {
        let dao_wallet = self.holders.get(&Holder::Dao).unwrap();

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();
        let (burn_pk, burn_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string()).unwrap();
        let (dao_exec_pk, dao_exec_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_EXEC_NS.to_string()).unwrap();
        let (dao_auth_xfer_pk, dao_auth_xfer_zkbin) = self
            .proving_keys
            .get(&DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_NS.to_string())
            .unwrap();
        let (dao_auth_xfer_enc_coin_pk, dao_auth_xfer_enc_coin_zkbin) = self
            .proving_keys
            .get(&DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_ENC_COIN_NS.to_string())
            .unwrap();

        let input_user_data_blind = Blind::random(&mut OsRng);
        let exec_signature_secret = SecretKey::random(&mut OsRng);

        assert!(!proposal_coinattrs.is_empty());
        let proposal_token_id = proposal_coinattrs[0].token_id;
        assert!(proposal_coinattrs.iter().all(|c| c.token_id == proposal_token_id));
        let proposal_amount = proposal_coinattrs.iter().map(|c| c.value).sum();

        let dao_coins = dao_wallet
            .unspent_money_coins
            .iter()
            .filter(|x| x.note.token_id == proposal_token_id)
            .cloned()
            .collect();
        let (spent_coins, change_value) = xfer::select_coins(dao_coins, proposal_amount)?;
        let tree = dao_wallet.money_merkle_tree.clone();

        let mut inputs = vec![];
        for coin in &spent_coins {
            let leaf_position = coin.leaf_position;
            let merkle_path = tree.witness(leaf_position, 0).unwrap();

            inputs.push(xfer::TransferCallInput {
                leaf_position,
                merkle_path,
                secret: coin.secret,
                note: coin.note.clone(),
                user_data_blind: input_user_data_blind,
            });
        }

        let mut outputs = vec![];
        for coin_attr in proposal_coinattrs.clone() {
            assert_eq!(proposal_token_id, coin_attr.token_id);
            outputs.push(coin_attr);
        }

        let spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();

        let dao_coin_attrs = CoinAttributes {
            public_key: dao_wallet.keypair.public,
            value: change_value,
            token_id: proposal_token_id,
            spend_hook,
            user_data: dao_bulla.inner(),
            blind: Blind::random(&mut OsRng),
        };
        outputs.push(dao_coin_attrs.clone());

        let xfer_builder = xfer::TransferCallBuilder {
            clear_inputs: vec![],
            inputs,
            outputs,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        };

        let (xfer_params, xfer_secrets) = xfer_builder.build()?;
        let mut data = vec![MoneyFunction::TransferV1 as u8];
        xfer_params.encode_async(&mut data).await?;
        let xfer_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // We need to extract stuff from the inputs and outputs that we'll also
        // use in the DAO::Exec call. This DAO API needs to be better.
        let mut input_value = 0;
        let mut input_value_blind = Blind::ZERO;
        for (input, blind) in spent_coins.iter().zip(xfer_secrets.input_value_blinds.iter()) {
            input_value += input.note.value;
            input_value_blind += *blind;
        }
        assert_eq!(
            pedersen_commitment_u64(input_value, input_value_blind),
            xfer_params.inputs.iter().map(|input| input.value_commit).sum()
        );

        let exec_builder = DaoExecCall {
            proposal: proposal.clone(),
            dao: dao.clone(),
            yes_vote_value,
            all_vote_value,
            yes_vote_blind,
            all_vote_blind,
            input_value,
            input_value_blind,
            input_user_data_blind,
            hook_dao_exec: DAO_CONTRACT_ID.inner(),
            signature_secret: exec_signature_secret,
        };

        let (exec_params, exec_proofs) = exec_builder.make(dao_exec_zkbin, dao_exec_pk)?;
        let mut data = vec![DaoFunction::Exec as u8];
        exec_params.encode_async(&mut data).await?;
        let exec_call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Auth module
        let auth_xfer_builder = DaoAuthMoneyTransferCall {
            proposal: proposal.clone(),
            proposal_coinattrs,
            dao: dao.clone(),
            input_user_data_blind,
            dao_coin_attrs,
        };
        let (auth_xfer_params, auth_xfer_proofs) = auth_xfer_builder.make(
            dao_auth_xfer_zkbin,
            dao_auth_xfer_pk,
            dao_auth_xfer_enc_coin_zkbin,
            dao_auth_xfer_enc_coin_pk,
        )?;
        let mut data = vec![DaoFunction::AuthMoneyTransfer as u8];
        auth_xfer_params.encode_async(&mut data).await?;
        let auth_xfer_call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // We need to construct this tree, where exec is the parent:
        //
        //   exec ->
        //       auth_xfer
        //       xfer
        //

        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: exec_call, proofs: exec_proofs },
            vec![
                DarkTree::new(
                    ContractCallLeaf { call: auth_xfer_call, proofs: auth_xfer_proofs },
                    vec![],
                    None,
                    None,
                ),
                DarkTree::new(
                    ContractCallLeaf { call: xfer_call, proofs: xfer_secrets.proofs },
                    vec![],
                    None,
                    None,
                ),
            ],
        )?;

        // If fees are enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let auth_xfer_sigs = vec![];
            let xfer_sigs = tx.create_sigs(&xfer_secrets.signature_secrets)?;
            let exec_sigs = tx.create_sigs(&[exec_signature_secret])?;
            tx.signatures = vec![auth_xfer_sigs, xfer_sigs, exec_sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            // Append the fee call to the transaction
            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with necessary keys.
        let mut tx = tx_builder.build()?;
        let auth_xfer_sigs = vec![];
        let xfer_sigs = tx.create_sigs(&xfer_secrets.signature_secrets)?;
        let exec_sigs = tx.create_sigs(&[exec_signature_secret])?;
        tx.signatures = vec![auth_xfer_sigs, xfer_sigs, exec_sigs];

        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, xfer_params, exec_params, fee_params))
    }

    /// Execute the transaction made by `dao_exec()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_dao_exec_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        xfer_params: &MoneyTransferParamsV1,
        _exec_params: &DaoExecParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u64,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        // Execute the transaction
        wallet.validator.add_transactions(&[tx], block_height, true, self.verify_fees).await?;

        if !append {
            return Ok(vec![])
        }

        let mut inputs = xfer_params.inputs.to_vec();
        let mut outputs = xfer_params.outputs.to_vec();

        if let Some(ref fee_params) = fee_params {
            inputs.push(fee_params.input.clone());
            outputs.push(fee_params.output.clone());
        }

        for input in inputs {
            if let Some(spent_coin) = wallet
                .unspent_money_coins
                .iter()
                .find(|x| x.nullifier() == input.nullifier)
                .cloned()
            {
                debug!("Found spent OwnCoin({}) for {:?}", spent_coin.coin, holder);
                wallet.unspent_money_coins.retain(|x| x.nullifier() != input.nullifier);
                wallet.spent_money_coins.push(spent_coin.clone());
            }
        }

        let mut found_owncoins = vec![];
        for output in outputs {
            wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));

            let Ok(note) = output.note.decrypt::<MoneyNote>(&wallet.keypair.secret) else {
                continue
            };

            let owncoin = OwnCoin {
                coin: output.coin,
                note: note.clone(),
                secret: wallet.keypair.secret,
                leaf_position: wallet.money_merkle_tree.mark().unwrap(),
            };

            debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
            wallet.unspent_money_coins.push(owncoin.clone());
            found_owncoins.push(owncoin);
        }

        Ok(found_owncoins)
    }
}
