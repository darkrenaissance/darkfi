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
    blockchain::{compute_fee, expected_reward},
    crypto::{
        contract_id::MONEY_CONTRACT_ID, note::AeadEncryptedNote, BaseBlind, FuncId, MerkleNode,
        MerkleTree, ScalarBlind, SecretKey,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use rand::rngs::OsRng;

#[test]
fn dep8() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Showcase DEP-0008 and how tx-local state works.

        // Initialize test harness
        use Holder::{Alice, Bob, Charlie};
        let mut th = TestHarness::new(&[Alice, Bob, Charlie], true).await?;

        // Generate one new block mined by Alice
        th.generate_block_all(&Alice).await?;

        // Generate two new blocks mined by Bob
        th.generate_block_all(&Bob).await?;
        th.generate_block_all(&Bob).await?;

        let current_block_height = 4;

        // Assert correct rewards
        let alice_coins = th.coins(&Alice);
        let bob_coins = th.coins(&Bob);
        assert_eq!(alice_coins.len(), 1);
        assert_eq!(bob_coins.len(), 2);
        assert_eq!(alice_coins[0].note.value, expected_reward(1));
        assert_eq!(bob_coins[0].note.value, expected_reward(2));
        assert_eq!(bob_coins[1].note.value, expected_reward(3));

        // Now we create an Alice to Charlie transfer call, where the
        // output is used to pay the fee.
        //
        // The transfer call change-output will be marked as tx-local.
        // which will allow it to be used as the fee call input which
        // is also marked as tx-local, signalling the correct verification
        // path in the contract's functions. The fee call output will
        // not be used further in the transaction, so it will not be
        // marked as tx-local - meaning it will get added to the global
        // on-chain state.

        // We'll clone the on-chain Merkle tree because we're creating
        // calls manually here.
        let alice_wallet = th.wallet(&Alice);
        let money_merkle_tree = alice_wallet.money_merkle_tree.clone();
        // For the tx-local tree, we'll initialize a new one.
        // It gets created the same way like the on-chain one, initialized
        // with a zero-leaf.
        let mut money_merkle_tree_local = MerkleTree::new(1);
        money_merkle_tree_local.append(MerkleNode::from(pallas::Base::ZERO));

        // TODO: Might be worth implementing an abstraction in test-harness
        // but also the contract client-side API should be more powerful.
        let (mint_pk, mint_zkbin) = th.proving_keys.get(MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let (burn_pk, burn_zkbin) = th.proving_keys.get(MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();

        let rcpt = th.wallet(&Charlie).keypair.public;

        // Manually create the call
        let (mut alice_xfer_params, secrets, _) = make_transfer_call(
            alice_wallet.keypair,
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

        // The idea is to use this call's output to pay the tx fee.
        // So we'll change the `Output` to `tx_Local = true`, and add
        // it to our tx-local Merkle tree so we can create the correct
        // inclusion proof.
        assert_eq!(alice_xfer_params.inputs.len(), 1);
        assert_eq!(alice_xfer_params.outputs.len(), 2);

        // Find the change output by trial and error. We do this because
        // in `make_transfer_call()` the outputs are shuffled.
        // Initialize output_coin with another coin to workaround the compiler.
        let mut output_coin = alice_coins[0].clone();
        for output in alice_xfer_params.outputs.iter_mut() {
            let Ok(note) = output.note.decrypt::<MoneyNote>(&alice_wallet.keypair.secret) else {
                continue
            };

            // In this tx, we want to use this change output to pay for
            // the transaction fee. We have to add it to the tx-local
            // Merkle tree in order to be able to provide a valid
            // inclusion proof.
            output.tx_local = true;
            money_merkle_tree_local.append(MerkleNode::from(output.coin.inner()));

            output_coin = OwnCoin {
                coin: output.coin,
                note: note.clone(),
                secret: alice_wallet.keypair.secret,
                leaf_position: money_merkle_tree_local.mark().unwrap(),
            };
            break
        }

        // Encode the Transfer call
        let mut data = vec![MoneyFunction::TransferV1 as u8];
        alice_xfer_params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the TransactionBuilder containing the Transfer call.
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: secrets.proofs }, vec![])?;

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&secrets.signature_secrets)?;
        tx.signatures = vec![sigs];

        // First we verify the fee-less transaction to see how much gas it
        // uses for execution and verification.
        let validator = alice_wallet.validator().read().await;
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
        let change_value = output_coin.note.value - required_fee;

        // Input and output setup.
        let input = FeeCallInput {
            coin: output_coin.clone(),
            merkle_path: money_merkle_tree_local.witness(output_coin.leaf_position, 0).unwrap(),
            user_data_blind: BaseBlind::random(&mut OsRng),
        };

        // The output will not be used further in the transaction so
        // it will not be marked as tx-local.
        let output = FeeCallOutput {
            public_key: alice_wallet.keypair.public,
            value: change_value,
            token_id: output_coin.note.token_id,
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
                // Here we mark the Input tx-local since the Ouput it
                // comes from (the previous Transfer call) creates it.
                tx_local: true,
            },
            output: Output {
                value_commit: public_inputs.output_value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.output_coin,
                note: encrypted_note,
                // We don't use this Output further in the tx, so we
                // don't mark it tx-local.
                tx_local: false,
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

        // Now build the actual transaction and sign it with all necessary keys
        let mut alice_tx = tx_builder.build()?;
        let sigs = alice_tx.create_sigs(&secrets.signature_secrets)?;
        alice_tx.signatures = vec![sigs];
        let sigs = alice_tx.create_sigs(&[signature_secret])?;
        alice_tx.signatures.push(sigs);

        th.execute_transfer_tx(
            &Alice,
            alice_tx.clone(),
            &alice_xfer_params,
            &alice_fee_params,
            current_block_height,
            true,
        )
        .await?;

        th.execute_transfer_tx(
            &Bob,
            alice_tx.clone(),
            &alice_xfer_params,
            &alice_fee_params,
            current_block_height,
            true,
        )
        .await?;

        th.execute_transfer_tx(
            &Charlie,
            alice_tx.clone(),
            &alice_xfer_params,
            &alice_fee_params,
            current_block_height,
            true,
        )
        .await?;

        // Thanks for reading
        Ok(())
    })
}
