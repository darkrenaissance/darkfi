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
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_dao_contract::{
    dao_client,
    dao_client::{DaoInfo, DaoProposalInfo, DaoVoteCall, DaoVoteInput},
    dao_model::DaoBlindAggregateVote,
    money_client, DaoFunction, DAO_CONTRACT_ZKAS_DAO_EXEC_NS, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};
use darkfi_money_contract::{
    client::OwnCoin, MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        pedersen_commitment_u64, Keypair, PublicKey, SecretKey, TokenId, DAO_CONTRACT_ID,
        MONEY_CONTRACT_ID,
    },
    incrementalmerkletree::Tree,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;

use super::Drk;
use crate::wallet_dao::{Dao, DaoProposal};

impl Drk {
    /// Mint a DAO on-chain
    pub async fn dao_mint(&self, dao_id: u64) -> Result<Transaction> {
        let dao = self.get_dao_by_id(dao_id).await?;

        if dao.tx_hash.is_some() {
            return Err(anyhow!("This DAO seems to have already been minted on-chain"))
        }

        let dao_info = DaoInfo {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_base: dao.approval_ratio_base,
            approval_ratio_quot: dao.approval_ratio_quot,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(dao_mint_zkbin) = zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_MINT_NS) else {
            return Err(anyhow!("DAO Mint circuit not found"));
        };

        let dao_mint_zkbin = ZkBinary::decode(&dao_mint_zkbin.1)?;
        let k = 13;
        let dao_mint_circuit =
            ZkCircuit::new(empty_witnesses(&dao_mint_zkbin), dao_mint_zkbin.clone());
        eprintln!("Creating DAO Mint proving key");
        let dao_mint_pk = ProvingKey::build(k, &dao_mint_circuit);

        let (params, proofs) =
            dao_client::make_mint_call(&dao_info, &dao.secret_key, &dao_mint_zkbin, &dao_mint_pk)?;

        let mut data = vec![DaoFunction::Mint as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *DAO_CONTRACT_ID, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &[dao.secret_key])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Create a DAO proposal
    pub async fn dao_propose(
        &self,
        dao_id: u64,
        recipient: PublicKey,
        amount: u64,
        token_id: TokenId,
    ) -> Result<Transaction> {
        let Ok(dao) = self.get_dao_by_id(dao_id).await else {
            return Err(anyhow!("DAO not found in wallet"))
        };

        if dao.leaf_position.is_none() || dao.tx_hash.is_none() {
            return Err(anyhow!("DAO seems to not have been deployed yet"))
        }

        let bulla = dao.bulla();
        let owncoins = self.get_coins(false).await?;

        let mut dao_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        dao_owncoins.retain(|x| {
            x.note.token_id == token_id &&
                x.note.spend_hook == DAO_CONTRACT_ID.inner() &&
                x.note.user_data == bulla.inner()
        });

        let mut gov_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        gov_owncoins.retain(|x| x.note.token_id == dao.gov_token_id);

        if dao_owncoins.is_empty() {
            return Err(anyhow!("Did not find any {} coins owned by this DAO", token_id))
        }

        if gov_owncoins.is_empty() {
            return Err(anyhow!("Did not find any governance {} coins in wallet", dao.gov_token_id))
        }

        if dao_owncoins.iter().map(|x| x.note.value).sum::<u64>() < amount {
            return Err(anyhow!("Not enough DAO balance for token ID: {}", token_id))
        }

        if gov_owncoins.iter().map(|x| x.note.value).sum::<u64>() < dao.proposer_limit {
            return Err(anyhow!("Not enough gov token {} balance to propose", dao.gov_token_id))
        }

        // FIXME: Here we're looking for a coin == proposer_limit but this shouldn't have to
        // be the case {
        let Some(gov_coin) = gov_owncoins.iter().find(|x| x.note.value == dao.proposer_limit) else {
            return Err(anyhow!("Did not find a single gov coin of value {}", dao.proposer_limit));
        };
        // }

        // Lookup the zkas bins
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(propose_burn_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS) else
        {
            return Err(anyhow!("Propose Burn circuit not found"))
        };

        let Some(propose_main_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS) else
        {
            return Err(anyhow!("Propose Main circuit not found"))
        };

        let propose_burn_zkbin = ZkBinary::decode(&propose_burn_zkbin.1)?;
        let propose_main_zkbin = ZkBinary::decode(&propose_main_zkbin.1)?;

        let k = 13;
        let propose_burn_circuit =
            ZkCircuit::new(empty_witnesses(&propose_burn_zkbin), propose_burn_zkbin.clone());
        let propose_main_circuit =
            ZkCircuit::new(empty_witnesses(&propose_main_zkbin), propose_main_zkbin.clone());

        eprintln!("Creating Propose Burn circuit proving key");
        let propose_burn_pk = ProvingKey::build(k, &propose_burn_circuit);
        eprintln!("Creating Propose Main circuit proving key");
        let propose_main_pk = ProvingKey::build(k, &propose_main_circuit);

        // Now create the parameters for the proposal tx
        let signature_secret = SecretKey::random(&mut OsRng);

        // Get the Merkle path for the gov coin in the money tree
        let money_merkle_tree = self.get_money_tree().await?;
        let root = money_merkle_tree.root(0).unwrap();
        let gov_coin_merkle_path =
            money_merkle_tree.authentication_path(gov_coin.leaf_position, &root).unwrap();

        // Fetch the daos Merkle tree
        let (daos_tree, _) = self.get_dao_trees().await?;

        let input = dao_client::DaoProposeStakeInput {
            secret: gov_coin.secret, // <-- TODO: Is this correct?
            note: gov_coin.note.clone(),
            leaf_position: gov_coin.leaf_position,
            merkle_path: gov_coin_merkle_path,
            signature_secret,
        };

        let (dao_merkle_path, dao_merkle_root) = {
            let root = daos_tree.root(0).unwrap();
            let leaf_pos = dao.leaf_position.unwrap();
            let dao_merkle_path = daos_tree.authentication_path(leaf_pos, &root).unwrap();
            (dao_merkle_path, root)
        };

        let proposal_blind = pallas::Base::random(&mut OsRng);
        let proposal = dao_client::DaoProposalInfo {
            dest: recipient,
            amount,
            serial: pallas::Base::random(&mut OsRng),
            token_id,
            blind: proposal_blind,
        };

        let daoinfo = DaoInfo {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_quot: dao.approval_ratio_quot,
            approval_ratio_base: dao.approval_ratio_base,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let call = dao_client::DaoProposeCall {
            inputs: vec![input],
            proposal,
            dao: daoinfo,
            dao_leaf_position: dao.leaf_position.unwrap(),
            dao_merkle_path,
            dao_merkle_root,
        };

        eprintln!("Creating ZK proofs...");
        let (params, proofs) = call.make(
            &propose_burn_zkbin,
            &propose_burn_pk,
            &propose_main_zkbin,
            &propose_main_pk,
        )?;

        let mut data = vec![DaoFunction::Propose as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *DAO_CONTRACT_ID, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &vec![signature_secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Vote on a DAO proposal
    pub async fn dao_vote(
        &self,
        dao_id: u64,
        proposal_id: u64,
        vote_option: bool,
        weight: u64,
    ) -> Result<Transaction> {
        let dao = self.get_dao_by_id(dao_id).await?;
        let proposals = self.get_dao_proposals(dao_id).await?;
        let Some(proposal) = proposals.iter().find(|x| x.id == proposal_id) else {
            return Err(anyhow!("Proposal ID not found"))
        };

        let money_tree = self.get_money_tree().await?;

        let mut coins: Vec<OwnCoin> =
            self.get_coins(false).await?.iter().map(|x| x.0.clone()).collect();

        coins.retain(|x| x.note.token_id == dao.gov_token_id);
        coins.retain(|x| x.note.spend_hook == pallas::Base::zero());

        if coins.iter().map(|x| x.note.value).sum::<u64>() < weight {
            return Err(anyhow!("Not enough balance for vote weight"))
        }

        // TODO: The spent coins need to either be marked as spent here, and/or on scan
        let mut spent_value = 0;
        let mut spent_coins = vec![];
        let mut inputs = vec![];
        let mut input_secrets = vec![];

        // FIXME: We don't take back any change so it's possible to vote with > requested weight.
        for coin in coins {
            if spent_value >= weight {
                break
            }

            spent_value += coin.note.value;
            spent_coins.push(coin.clone());

            let signature_secret = SecretKey::random(&mut OsRng);
            input_secrets.push(signature_secret);

            let root = money_tree.root(0).unwrap();
            let leaf_position = coin.leaf_position;
            let merkle_path = money_tree.authentication_path(coin.leaf_position, &root).unwrap();

            let input = DaoVoteInput {
                secret: coin.secret,
                note: coin.note.clone(),
                leaf_position,
                merkle_path,
                signature_secret,
            };

            inputs.push(input);
        }

        // We use the DAO secret to encrypt the vote.
        let vote_keypair = Keypair::new(dao.secret_key);

        let proposal_info = DaoProposalInfo {
            dest: proposal.recipient,
            amount: proposal.amount,
            serial: proposal.serial,
            token_id: proposal.token_id,
            blind: proposal.bulla_blind,
        };

        let dao_info = DaoInfo {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_quot: dao.approval_ratio_quot,
            approval_ratio_base: dao.approval_ratio_base,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let call = DaoVoteCall {
            inputs,
            vote_option,
            yes_vote_blind: pallas::Scalar::random(&mut OsRng),
            vote_keypair,
            proposal: proposal_info,
            dao: dao_info,
        };

        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(dao_vote_burn_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS) else
        {
            return Err(anyhow!("DAO Vote Burn circuit not found"))
        };

        let Some(dao_vote_main_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS) else
        {
            return Err(anyhow!("DAO Vote Main circuit not found"))
        };

        let dao_vote_burn_zkbin = ZkBinary::decode(&dao_vote_burn_zkbin.1)?;
        let dao_vote_main_zkbin = ZkBinary::decode(&dao_vote_main_zkbin.1)?;

        let k = 13;
        let dao_vote_burn_circuit =
            ZkCircuit::new(empty_witnesses(&dao_vote_burn_zkbin), dao_vote_burn_zkbin.clone());
        let dao_vote_main_circuit =
            ZkCircuit::new(empty_witnesses(&dao_vote_main_zkbin), dao_vote_main_zkbin.clone());

        eprintln!("Creating DAO Vote Burn proving key");
        let dao_vote_burn_pk = ProvingKey::build(k, &dao_vote_burn_circuit);
        eprintln!("Creating DAO Vote Main proving key");
        let dao_vote_main_pk = ProvingKey::build(k, &dao_vote_main_circuit);

        let (params, proofs) = call.make(
            &dao_vote_burn_zkbin,
            &dao_vote_burn_pk,
            &dao_vote_main_zkbin,
            &dao_vote_main_pk,
        )?;

        let mut data = vec![DaoFunction::Vote as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *DAO_CONTRACT_ID, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &input_secrets)?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Import given DAO votes into the wallet
    /// This function is really bad but I'm also really tired and annoyed.
    pub async fn dao_exec(&self, dao: Dao, proposal: DaoProposal) -> Result<Transaction> {
        let dao_bulla = dao.bulla();
        eprintln!("Fetching proposal's votes");
        let votes = self.get_dao_proposal_votes(proposal.id).await?;

        // Find the treasury coins that can be used for this proposal
        let mut coins: Vec<OwnCoin> =
            self.get_coins(false).await?.iter().map(|x| x.0.clone()).collect();
        coins.retain(|x| x.note.spend_hook == DAO_CONTRACT_ID.inner());
        coins.retain(|x| x.note.user_data == dao_bulla.inner());
        coins.retain(|x| x.note.token_id == proposal.token_id);

        if coins.iter().map(|x| x.note.value).sum::<u64>() < proposal.amount {
            return Err(anyhow!("Not enough balance in DAO treasury to execute proposal"))
        }

        // Used to export user_data from this coin so it can be accessed by DAO::exec()
        let user_data_blind = pallas::Base::random(&mut OsRng);

        let user_serial = pallas::Base::random(&mut OsRng);
        let user_coin_blind = pallas::Base::random(&mut OsRng);
        let dao_serial = pallas::Base::random(&mut OsRng);
        let dao_coin_blind = pallas::Base::random(&mut OsRng);

        // TODO: FIXME: Clean this up and create an API
        let exec_signature_secret = SecretKey::random(&mut OsRng);
        let mut xfer_signature_secrets = vec![];
        let mut xfer_inputs = vec![];

        let mut input_coins = vec![];
        let mut input_amount = 0;
        for coin in coins {
            input_amount += coin.note.value;
            input_coins.push(coin);
            if input_amount >= proposal.amount {
                break
            }
        }

        let money_merkle_tree = self.get_money_tree().await?;
        let money_merkle_root = money_merkle_tree.root(0).unwrap();

        let mut input_value_blind = pallas::Scalar::from(0);
        for coin in &input_coins {
            let value_blind = pallas::Scalar::random(&mut OsRng);
            let sig_secret = SecretKey::random(&mut OsRng);
            xfer_signature_secrets.push(sig_secret);

            xfer_inputs.push(money_client::TransferInput {
                leaf_position: coin.leaf_position,
                merkle_path: money_merkle_tree
                    .authentication_path(coin.leaf_position, &money_merkle_root)
                    .unwrap(),
                secret: dao.secret_key,
                note: coin.note.clone(),
                user_data_blind,
                value_blind,
                signature_secret: sig_secret,
            });

            input_value_blind += value_blind;
        }

        let input_sum = input_coins.iter().map(|x| x.note.value).sum::<u64>();

        // I'm tired
        let xfer_outputs = vec![
            // Proposal send
            money_client::TransferOutput {
                value: proposal.amount,
                token_id: proposal.token_id,
                public: proposal.recipient,
                serial: proposal.serial,
                coin_blind: proposal.bulla_blind,
                spend_hook: pallas::Base::zero(),
                user_data: pallas::Base::zero(),
            },
            // Change
            money_client::TransferOutput {
                value: input_sum - proposal.amount,
                token_id: proposal.token_id,
                public: PublicKey::from_secret(dao.secret_key),
                serial: dao_serial,
                coin_blind: dao_coin_blind,
                spend_hook: DAO_CONTRACT_ID.inner(),
                user_data: dao_bulla.inner(),
            },
        ];

        let xfer_call = money_client::TransferCall {
            clear_inputs: vec![],
            inputs: xfer_inputs,
            outputs: xfer_outputs,
        };

        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;
        let Some(mint_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_MINT_NS_V1) else {
            return Err(anyhow!("Money Mint circuit not found"))
        };
        let Some(burn_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_BURN_NS_V1) else {
            return Err(anyhow!("Money Burn circuit not found"))
        };
        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin.1)?;
        let k = 13;
        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin), mint_zkbin.clone());
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin), burn_zkbin.clone());
        eprintln!("Creating Money Mint circuit proving key");
        let mint_pk = ProvingKey::build(k, &mint_circuit);
        eprintln!("Creating Money Burn circuit proving key");
        let burn_pk = ProvingKey::build(k, &burn_circuit);

        let (xfer_params, xfer_proofs) =
            xfer_call.make(&mint_zkbin, &mint_pk, &burn_zkbin, &burn_pk)?;

        let mut data = vec![MoneyFunction::Transfer as u8];
        xfer_params.encode(&mut data)?;
        let xfer_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(exec_zkbin) = zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_EXEC_NS) else {
            return Err(anyhow!("DAO Exec circuit not found"))
        };
        let exec_zkbin = ZkBinary::decode(&exec_zkbin.1)?;
        let exec_circuit = ZkCircuit::new(empty_witnesses(&exec_zkbin), exec_zkbin.clone());
        eprintln!("Creating DAO Exec circuit proving key");
        let exec_pk = ProvingKey::build(k, &exec_circuit);

        // Count votes
        let mut total_yes_vote_value = 0;
        let mut total_all_vote_value = 0;
        let mut blind_total_vote = DaoBlindAggregateVote::default();
        let mut total_yes_vote_blind = pallas::Scalar::zero();
        let mut total_all_vote_blind = pallas::Scalar::zero();

        for (_, vote) in votes.iter().enumerate() {
            total_yes_vote_blind += vote.yes_vote_blind;
            total_all_vote_blind += vote.all_vote_blind;

            let yes_vote_value = vote.vote_option as u64 * vote.all_vote_value;
            eprintln!("yes_vote = {}", yes_vote_value);
            total_yes_vote_value += yes_vote_value;
            total_all_vote_value += vote.all_vote_value;

            let yes_vote_commit = pedersen_commitment_u64(yes_vote_value, vote.yes_vote_blind);
            let all_vote_commit = pedersen_commitment_u64(vote.all_vote_value, vote.all_vote_blind);

            let blind_vote = DaoBlindAggregateVote { yes_vote_commit, all_vote_commit };
            blind_total_vote.aggregate(blind_vote);
        }

        eprintln!("yes = {}, all = {}", total_yes_vote_value, total_all_vote_value);

        let prop_t = DaoProposalInfo {
            dest: proposal.recipient,
            amount: proposal.amount,
            serial: proposal.serial,
            token_id: proposal.token_id,
            blind: proposal.bulla_blind, // <-- FIXME: wtf
        };

        // TODO: user blind weirdness in proposal
        // TODO: allvote/yesvote is 11 weirdly

        let dao_t = DaoInfo {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_quot: dao.approval_ratio_quot,
            approval_ratio_base: dao.approval_ratio_base,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let dao_exec_call = dao_client::DaoExecCall {
            proposal: prop_t,
            dao: dao_t,
            yes_vote_value: total_yes_vote_value,
            all_vote_value: total_all_vote_value,
            yes_vote_blind: total_yes_vote_blind,
            all_vote_blind: total_all_vote_blind,
            user_serial,
            user_coin_blind,
            dao_serial,
            dao_coin_blind,
            input_value: input_sum, // <-- FIXME
            input_value_blind,      // <-- FIXME
            hook_dao_exec: DAO_CONTRACT_ID.inner(),
            signature_secret: exec_signature_secret,
        };

        let (exec_params, exec_proofs) = dao_exec_call.make(&exec_zkbin, &exec_pk)?;

        let mut data = vec![DaoFunction::Exec as u8];
        exec_params.encode(&mut data)?;
        let exec_call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        let mut tx = Transaction {
            calls: vec![xfer_call, exec_call],
            proofs: vec![xfer_proofs, exec_proofs],
            signatures: vec![],
        };

        let xfer_sigs = tx.create_sigs(&mut OsRng, &xfer_signature_secrets)?;
        let exec_sigs = tx.create_sigs(&mut OsRng, &[exec_signature_secret])?;
        tx.signatures = vec![xfer_sigs, exec_sigs];

        Ok(tx)
    }
}
