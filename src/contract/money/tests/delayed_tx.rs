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

use darkfi::{
    tx::{ContractCallLeaf, TransactionBuilder},
    validator::fees::compute_fee,
    zk::halo2::Field,
    Result,
};
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_money_contract::{
    client::{
        compute_remainder_blind,
        fee_v1::{create_fee_proof, FeeCallInput, FeeCallOutput, FEE_CALL_GAS},
        transfer_v1::make_transfer_call,
        MoneyNote, OwnCoin,
    },
    model::{Input, MoneyFeeParamsV1, Output},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_FEE_NS_V1,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    blockchain::expected_reward,
    crypto::{
        contract_id::MONEY_CONTRACT_ID, note::AeadEncryptedNote, BaseBlind, FuncId, MerkleNode,
        ScalarBlind, SecretKey,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use rand::rngs::OsRng;

#[test]
#[ignore]
fn delayed_tx() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 3] = [Holder::Alice, Holder::Bob, Holder::Charlie];

        // Initialize harness
        let mut th = TestHarness::new(&HOLDERS, true).await?;

        // Generate one new block mined by Alice
        th.generate_block(&Holder::Alice, &HOLDERS).await?;

        // Generate two new blocks mined by Bob
        th.generate_block(&Holder::Bob, &HOLDERS).await?;
        th.generate_block(&Holder::Bob, &HOLDERS).await?;

        // Assert correct rewards
        let alice_coins = &th.holders.get(&Holder::Alice).unwrap().unspent_money_coins;
        let bob_coins = th.holders.get(&Holder::Bob).unwrap().unspent_money_coins.clone();
        assert!(alice_coins.len() == 1);
        assert!(bob_coins.len() == 2);
        assert!(alice_coins[0].note.value == expected_reward(1));
        assert!(bob_coins[0].note.value == expected_reward(2));
        assert!(bob_coins[1].note.value == expected_reward(3));

        let current_block_height = 4;

        // Manually create an Alice to Charlie transfer call,
        // where the output is used to pay the fee
        let wallet = th.holders.get(&Holder::Alice).unwrap();
        let rcpt = th.holders.get(&Holder::Charlie).unwrap().keypair.public;
        let mut money_merkle_tree = wallet.money_merkle_tree.clone();

        let (mint_pk, mint_zkbin) = th.proving_keys.get(MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let (burn_pk, burn_zkbin) = th.proving_keys.get(MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();

        // Create the transfer call
        let (alice_xfer_params, secrets, _) = make_transfer_call(
            wallet.keypair,
            rcpt,
            alice_coins[0].note.value / 2,
            alice_coins[0].note.token_id,
            alice_coins.to_owned(),
            money_merkle_tree.clone(),
            None,
            None,
            mint_zkbin.clone(),
            mint_pk.clone(),
            burn_zkbin.clone(),
            burn_pk.clone(),
            false,
        )?;

        let mut output_coins = vec![];
        for output in &alice_xfer_params.outputs {
            money_merkle_tree.append(MerkleNode::from(output.coin.inner()));

            // Attempt to decrypt the output note to see if this is a coin for the holder.
            let Ok(note) = output.note.decrypt::<MoneyNote>(&wallet.keypair.secret) else {
                continue
            };

            let owncoin = OwnCoin {
                coin: output.coin,
                note: note.clone(),
                secret: wallet.keypair.secret,
                leaf_position: money_merkle_tree.mark().unwrap(),
            };

            output_coins.push(owncoin);
        }

        // Encode the call
        let mut data = vec![MoneyFunction::TransferV1 as u8];
        alice_xfer_params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the TransactionBuilder containing the `Transfer` call
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: secrets.proofs }, vec![])?;

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&secrets.signature_secrets)?;
        tx.signatures = vec![sigs];

        // First we verify the fee-less transaction to see how much gas it uses for execution
        // and verification.
        let validator = wallet.validator.read().await;
        let gas_used = validator
            .add_test_transactions(
                &[tx],
                current_block_height,
                validator.consensus.module.target,
                false,
                false,
            )
            .await?
            .0;
        drop(validator);

        // Compute the required fee
        let required_fee = compute_fee(&(gas_used + FEE_CALL_GAS));

        let coin = &output_coins[0];
        let change_value = coin.note.value - required_fee;

        // Input and output setup
        let input = FeeCallInput {
            coin: coin.clone(),
            merkle_path: money_merkle_tree.witness(coin.leaf_position, 0).unwrap(),
            user_data_blind: BaseBlind::random(&mut OsRng),
        };

        let output = FeeCallOutput {
            public_key: wallet.keypair.public,
            value: change_value,
            token_id: coin.note.token_id,
            blind: BaseBlind::random(&mut OsRng),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
        };

        // Create blinding factors
        let token_blind = BaseBlind::random(&mut OsRng);
        let input_value_blind = ScalarBlind::random(&mut OsRng);
        let fee_value_blind = ScalarBlind::random(&mut OsRng);
        let output_value_blind = compute_remainder_blind(&[input_value_blind], &[fee_value_blind]);

        // Create an ephemeral signing key
        let signature_secret = SecretKey::random(&mut OsRng);

        let (fee_pk, fee_zkbin) = th.proving_keys.get(MONEY_CONTRACT_ZKAS_FEE_NS_V1).unwrap();

        let (proof, public_inputs) = create_fee_proof(
            fee_zkbin,
            fee_pk,
            &input,
            input_value_blind,
            &output,
            output_value_blind,
            output.spend_hook,
            output.user_data,
            output.blind,
            token_blind,
            signature_secret,
        )?;

        // Encrypted note for the output
        let note = MoneyNote {
            coin_blind: output.blind,
            value: output.value,
            token_id: output.token_id,
            spend_hook: output.spend_hook,
            user_data: output.user_data,
            value_blind: output_value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let fee_call_params = MoneyFeeParamsV1 {
            input: Input {
                value_commit: public_inputs.input_value_commit,
                token_commit: public_inputs.token_commit,
                nullifier: public_inputs.nullifier,
                merkle_root: public_inputs.merkle_root,
                user_data_enc: public_inputs.input_user_data_enc,
                signature_public: public_inputs.signature_public,
            },
            output: Output {
                value_commit: public_inputs.output_value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.output_coin,
                note: encrypted_note,
            },
            fee_value_blind,
            token_blind,
        };

        // Encode the contract call
        let mut data = vec![MoneyFunction::FeeV1 as u8];
        required_fee.encode_async(&mut data).await?;
        fee_call_params.encode_async(&mut data).await?;
        let fee_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: vec![proof] }, vec![])?;
        let alice_fee_params = Some(fee_call_params);

        // Now build the actual transaction and sign it with all necessary keys.
        let mut alice_tx = tx_builder.build()?;
        let sigs = alice_tx.create_sigs(&secrets.signature_secrets)?;
        alice_tx.signatures = vec![sigs];
        let sigs = alice_tx.create_sigs(&[signature_secret])?;
        alice_tx.signatures.push(sigs);

        // Bob transfers some tokens to Charlie
        let (bob_tx, (bob_xfer_params, bob_fee_params), _spent_soins) = th
            .transfer(
                bob_coins[0].note.value,
                &Holder::Bob,
                &Holder::Charlie,
                &[bob_coins[0].clone()],
                bob_coins[0].note.token_id,
                current_block_height,
                false,
            )
            .await?;

        // Bob->Charlie transaction gets in first
        for holder in &HOLDERS {
            th.execute_transfer_tx(
                holder,
                bob_tx.clone(),
                &bob_xfer_params,
                &bob_fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        // Execute the Alice->Charlie transaction
        for holder in &HOLDERS {
            th.execute_transfer_tx(
                holder,
                alice_tx.clone(),
                &alice_xfer_params,
                &alice_fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        // Assert coins in wallets
        let alice_coins = &th.holders.get(&Holder::Alice).unwrap().unspent_money_coins;
        let bob_coins = &th.holders.get(&Holder::Bob).unwrap().unspent_money_coins;
        let charlie_coins = &th.holders.get(&Holder::Charlie).unwrap().unspent_money_coins;
        assert!(alice_coins.len() == 1);
        assert!(bob_coins.len() == 1);
        assert!(charlie_coins.len() == 2);
        assert!(charlie_coins[0].note.value == expected_reward(2));
        assert!(charlie_coins[1].note.value == expected_reward(1) / 2);

        // Thanks for reading
        Ok(())
    })
}
