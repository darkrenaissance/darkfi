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

use std::collections::HashMap;

use log::{debug, error, info};
use smol::io::Cursor;
use tinyjson::JsonValue;

use darkfi::{
    blockchain::{
        BlockInfo, BlockchainOverlay, HeaderHash, SLED_PENDING_TX_ORDER_TREE, SLED_PENDING_TX_TREE,
        SLED_TX_LOCATION_TREE, SLED_TX_TREE,
    },
    error::TxVerifyFailed,
    runtime::vm_runtime::Runtime,
    tx::Transaction,
    util::time::Timestamp,
    validator::fees::{circuit_gas_use, GasData, PALLAS_SCHNORR_SIGNATURE_FEE},
    zk::VerifyingKey,
    Error, Result,
};
use darkfi_sdk::{
    crypto::{ContractId, PublicKey},
    deploy::DeployParamsV1,
    pasta::pallas,
    tx::TransactionHash,
};
use darkfi_serial::{deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable};

use crate::ExplorerService;

#[derive(Debug, Clone)]
/// Structure representing a `TRANSACTIONS_TABLE` record.
pub struct TransactionRecord {
    /// Transaction hash identifier
    pub transaction_hash: String,
    /// Header hash identifier of the block this transaction was included in
    pub header_hash: String,
    // TODO: Split the payload into a more easily readable fields
    /// Transaction payload
    pub payload: Transaction,
    /// Time transaction was added to the block
    pub timestamp: Timestamp,
    /// Total gas used for processing transaction
    pub total_gas_used: u64,
    /// Gas used by WASM
    pub wasm_gas_used: u64,
    /// Gas used by ZK circuit operations
    pub zk_circuit_gas_used: u64,
    /// Gas used for creating the transaction signature
    pub signature_gas_used: u64,
    /// Gas used for deployments
    pub deployment_gas_used: u64,
}

impl TransactionRecord {
    /// Auxiliary function to convert a `TransactionRecord` into a `JsonValue` array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::String(self.transaction_hash.clone()),
            JsonValue::String(self.header_hash.clone()),
            JsonValue::String(format!("{:?}", self.payload)),
            JsonValue::String(self.timestamp.to_string()),
            JsonValue::Number(self.total_gas_used as f64),
            JsonValue::Number(self.wasm_gas_used as f64),
            JsonValue::Number(self.zk_circuit_gas_used as f64),
            JsonValue::Number(self.signature_gas_used as f64),
            JsonValue::Number(self.deployment_gas_used as f64),
        ])
    }
}

impl ExplorerService {
    /// Resets transactions in the database by clearing transaction-related trees, returning an Ok result on success.
    pub fn reset_transactions(&self) -> Result<()> {
        // Initialize transaction trees to reset
        let trees_to_reset =
            [SLED_TX_TREE, SLED_TX_LOCATION_TREE, SLED_PENDING_TX_TREE, SLED_PENDING_TX_ORDER_TREE];

        // Iterate over each associated transaction tree and delete its contents
        for tree_name in &trees_to_reset {
            let tree = &self.db.blockchain.sled_db.open_tree(tree_name)?;
            tree.clear()?;
            let tree_name_str = std::str::from_utf8(tree_name)?;
            info!(target: "blockchain-explorer::blocks", "Successfully reset transaction tree: {tree_name_str}");
        }

        Ok(())
    }

    /// Provides the transaction count of all the transactions in the explorer database.
    pub fn get_transaction_count(&self) -> usize {
        self.db.blockchain.txs_len()
    }

    /// Fetches all known transactions from the database.
    ///
    /// This function retrieves all transactions stored in the database and transforms
    /// them into a vector of [`TransactionRecord`]s. If no transactions are found,
    /// it returns an empty vector.
    pub fn get_transactions(&self) -> Result<Vec<TransactionRecord>> {
        // Retrieve all transactions and handle any errors encountered
        let txs = self.db.blockchain.transactions.get_all().map_err(|e| {
            Error::DatabaseError(format!("[get_transactions] Trxs retrieval: {e:?}"))
        })?;

        // Transform the found `Transactions` into a vector of `TransactionRecords`
        let txs_records = txs
            .iter()
            .map(|(_, tx)| self.to_tx_record(None, tx))
            .collect::<Result<Vec<TransactionRecord>>>()?;

        Ok(txs_records)
    }

    /// Fetches all transactions from the database for the given block `header_hash`.
    ///
    /// This function retrieves all transactions associated with the specified
    /// block header hash. It first parses the header hash and then fetches
    /// the corresponding [`BlockInfo`]. If the block is found, it transforms its
    /// transactions into a vector of [`TransactionRecord`]s. If no transactions
    /// are found, it returns an empty vector.
    pub fn get_transactions_by_header_hash(
        &self,
        header_hash: &str,
    ) -> Result<Vec<TransactionRecord>> {
        // Parse header hash, returning an error if parsing fails
        let header_hash = header_hash
            .parse::<HeaderHash>()
            .map_err(|_| Error::ParseFailed("[get_transactions_by_header_hash] Invalid hash"))?;

        // Fetch block by hash and handle encountered errors
        let block = match self.db.blockchain.get_blocks_by_hash(&[header_hash]) {
            Ok(blocks) => blocks.first().cloned().unwrap(),
            Err(Error::BlockNotFound(_)) => return Ok(vec![]),
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_transactions_by_header_hash] Block retrieval failed: {e:?}"
                )))
            }
        };

        // Transform block transactions into transaction records
        block
            .txs
            .iter()
            .map(|tx| self.to_tx_record(self.get_block_info(block.header.hash())?, tx))
            .collect::<Result<Vec<TransactionRecord>>>()
    }

    /// Fetches a transaction given its header hash.
    ///
    /// This function retrieves the transaction associated with the provided
    /// [`TransactionHash`] and transforms it into a [`TransactionRecord`] if found.
    /// If no transaction is found, it returns `None`.
    pub fn get_transaction_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> Result<Option<TransactionRecord>> {
        let tx_store = &self.db.blockchain.transactions;

        // Attempt to retrieve the transaction using the provided hash handling any potential errors
        let tx_opt = &tx_store.get(&[*tx_hash], false).map_err(|e| {
            Error::DatabaseError(format!(
                "[get_transaction_by_hash] Transaction retrieval failed: {e:?}"
            ))
        })?[0];

        // Transform `Transaction` to a `TransactionRecord`, returning None if no transaction was found
        tx_opt.as_ref().map(|tx| self.to_tx_record(None, tx)).transpose()
    }

    /// Fetches the [`BlockInfo`] associated with a given transaction hash.
    ///
    /// This auxiliary function first fetches the location of the transaction in the blockchain.
    /// If the location is found, it retrieves the associated [`HeaderHash`] and then fetches
    /// the block information corresponding to that header hash. The function returns the
    /// [`BlockInfo`] if successful, or `None` if no location or header hash is found.
    fn get_tx_block_info(&self, tx_hash: &TransactionHash) -> Result<Option<BlockInfo>> {
        // Retrieve the location of the transaction
        let location =
            self.db.blockchain.transactions.get_location(&[*tx_hash], false).map_err(|e| {
                Error::DatabaseError(format!(
                    "[get_tx_block_info] Location retrieval failed: {e:?}"
                ))
            })?[0];

        // Fetch the `HeaderHash` associated with the location
        let header_hash = match location {
            None => return Ok(None),
            Some((block_height, _)) => {
                self.db.blockchain.blocks.get_order(&[block_height], false).map_err(|e| {
                    Error::DatabaseError(format!(
                        "[get_tx_block_info] Block retrieval failed: {e:?}"
                    ))
                })?[0]
            }
        };

        // Return the associated `BlockInfo` if the header hash is found; otherwise, return `None`.
        match header_hash {
            None => Ok(None),
            Some(header_hash) => self.get_block_info(header_hash).map_err(|e| {
                Error::DatabaseError(format!(
                    "[get_tx_block_info] BlockInfo retrieval failed: {e:?}"
                ))
            }),
        }
    }

    /// Fetches the [`BlockInfo`] associated with a given [`HeaderHash`].
    ///
    /// This auxiliary function attempts to retrieve the block information using
    /// the specified [`HeaderHash`]. It returns the associated [`BlockInfo`] if found,
    /// or `None` when not found.
    fn get_block_info(&self, header_hash: HeaderHash) -> Result<Option<BlockInfo>> {
        match self.db.blockchain.get_blocks_by_hash(&[header_hash]) {
            Err(Error::BlockNotFound(_)) => Ok(None),
            Ok(block_info) => Ok(block_info.into_iter().next()),
            Err(e) => Err(Error::DatabaseError(format!(
                "[get_transactions_by_header_hash] Block retrieval failed: {e:?}"
            ))),
        }
    }

    /// Calculates the gas data for a given transaction, returning a [`GasData`] instance detailing
    /// various aspects of the gas usage.
    pub async fn calculate_tx_gas_data(
        &self,
        tx: &Transaction,
        verify_fee: bool,
    ) -> Result<GasData> {
        let tx_hash = tx.hash();

        let overlay = BlockchainOverlay::new(&self.db.blockchain)?;

        // Gas accumulators
        let mut total_gas_used = 0;
        let mut zk_circuit_gas_used = 0;
        let mut wasm_gas_used = 0;
        let mut deploy_gas_used = 0;
        let mut gas_paid = 0;

        // Table of public inputs used for ZK proof verification
        let mut zkp_table = vec![];
        // Table of public keys used for signature verification
        let mut sig_table = vec![];

        // Index of the Fee-paying call
        let fee_call_idx = 0;

        // Map of ZK proof verifying keys for the transaction
        let mut verifying_keys: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();
        for call in &tx.calls {
            verifying_keys.insert(call.data.contract_id.to_bytes(), HashMap::new());
        }

        let block_target = self.db.blockchain.blocks.get_last()?.0 + 1;

        // We'll also take note of all the circuits in a Vec so we can calculate their verification cost.
        let mut circuits_to_verify = vec![];

        // Iterate over all calls to get the metadata
        for (idx, call) in tx.calls.iter().enumerate() {
            // Transaction must not contain a Money::PoWReward(0x02) call
            if call.data.is_money_pow_reward() {
                error!(target: "block_explorer::calculate_tx_gas_data", "Reward transaction detected");
                return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
            }

            // Write the actual payload data
            let mut payload = vec![];
            tx.calls.encode_async(&mut payload).await?;

            let wasm = overlay.lock().unwrap().contracts.get(call.data.contract_id)?;

            let mut runtime = Runtime::new(
                &wasm,
                overlay.clone(),
                call.data.contract_id,
                block_target,
                block_target,
                tx_hash,
                idx as u8,
            )?;

            let metadata = runtime.metadata(&payload)?;

            // Decode the metadata retrieved from the execution
            let mut decoder = Cursor::new(&metadata);

            // The tuple is (zkas_ns, public_inputs)
            let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
                AsyncDecodable::decode_async(&mut decoder).await?;
            let sig_pub: Vec<PublicKey> = AsyncDecodable::decode_async(&mut decoder).await?;

            if decoder.position() != metadata.len() as u64 {
                error!(
                    target: "block_explorer::calculate_tx_gas_data",
                    "[BLOCK_EXPLORER] Failed decoding entire metadata buffer for {}:{}", tx_hash, idx,
                );
                return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
            }

            // Here we'll look up verifying keys and insert them into the per-contract map.
            for (zkas_ns, _) in &zkp_pub {
                let inner_vk_map =
                    verifying_keys.get_mut(&call.data.contract_id.to_bytes()).unwrap();

                // TODO: This will be a problem in case of ::deploy, unless we force a different
                // namespace and disable updating existing circuit. Might be a smart idea to do
                // so in order to have to care less about being able to verify historical txs.
                if inner_vk_map.contains_key(zkas_ns.as_str()) {
                    continue
                }

                let (zkbin, vk) =
                    overlay.lock().unwrap().contracts.get_zkas(&call.data.contract_id, zkas_ns)?;

                inner_vk_map.insert(zkas_ns.to_string(), vk);
                circuits_to_verify.push(zkbin);
            }

            zkp_table.push(zkp_pub);
            sig_table.push(sig_pub);

            // Contracts are not included within blocks. They need to be deployed off-chain so that they can be accessed and utilized for fee data computation
            if call.data.is_deployment()
            /* DeployV1 */
            {
                // Deserialize the deployment parameters
                let deploy_params: DeployParamsV1 = deserialize_async(&call.data.data[1..]).await?;
                let deploy_cid = ContractId::derive_public(deploy_params.public_key);

                // Instantiate the new deployment runtime
                let mut deploy_runtime = Runtime::new(
                    &deploy_params.wasm_bincode,
                    overlay.clone(),
                    deploy_cid,
                    block_target,
                    block_target,
                    tx_hash,
                    idx as u8,
                )?;

                deploy_runtime.deploy(&deploy_params.ix)?;

                deploy_gas_used = deploy_runtime.gas_used();

                // Append the used deployment gas
                total_gas_used += deploy_gas_used;
            }

            // At this point we're done with the call and move on to the next one.
            // Accumulate the WASM gas used.
            wasm_gas_used = runtime.gas_used();

            // Append the used wasm gas
            total_gas_used += wasm_gas_used;
        }

        // The signature fee is tx_size + fixed_sig_fee * n_signatures
        let signature_gas_used = (PALLAS_SCHNORR_SIGNATURE_FEE * tx.signatures.len() as u64) +
            serialize_async(tx).await.len() as u64;

        // Append the used signature gas
        total_gas_used += signature_gas_used;

        // The ZK circuit fee is calculated using a function in validator/fees.rs
        for zkbin in circuits_to_verify.iter() {
            zk_circuit_gas_used = circuit_gas_use(zkbin);

            // Append the used zk circuit gas
            total_gas_used += zk_circuit_gas_used;
        }

        if verify_fee {
            // Deserialize the fee call to find the paid fee
            let fee: u64 = match deserialize_async(&tx.calls[fee_call_idx].data.data[1..9]).await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "block_explorer::calculate_tx_gas_data",
                        "[VALIDATOR] Failed deserializing tx {} fee call: {}", tx_hash, e,
                    );
                    return Err(TxVerifyFailed::InvalidFee.into())
                }
            };

            // TODO: This counts 1 gas as 1 token unit. Pricing should be better specified.
            // Check that enough fee has been paid for the used gas in this transaction.
            if total_gas_used > fee {
                error!(
                    target: "block_explorer::calculate_tx_gas_data",
                    "[VALIDATOR] Transaction {} has insufficient fee. Required: {}, Paid: {}",
                    tx_hash, total_gas_used, fee,
                );
                return Err(TxVerifyFailed::InsufficientFee.into())
            }
            debug!(target: "block_explorer::calculate_tx_gas_data", "The gas paid for transaction {}: {}", tx_hash, gas_paid);

            // Store paid fee
            gas_paid = fee;
        }

        // Commit changes made to the overlay
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        let fee_data = GasData {
            paid: gas_paid,
            wasm: wasm_gas_used,
            zk_circuits: zk_circuit_gas_used,
            signatures: signature_gas_used,
            deployments: deploy_gas_used,
        };

        debug!(target: "block_explorer::calculate_tx_gas_data", "The total gas usage for transaction {}: {:?}", tx_hash, fee_data);

        Ok(fee_data)
    }

    /// Converts a [`Transaction`] and its associated block information into a [`TransactionRecord`].
    ///
    /// This auxiliary function first retrieves the gas data associated with the provided transaction.
    /// If [`BlockInfo`] is not provided, it attempts to fetch it using the transaction's hash,
    /// returning an error if the block information cannot be found. Upon success, the function
    /// returns a [`TransactionRecord`] containing relevant details about the transaction.
    fn to_tx_record(
        &self,
        block_info_opt: Option<BlockInfo>,
        tx: &Transaction,
    ) -> Result<TransactionRecord> {
        // Fetch the gas data associated with the transaction
        let gas_data_option = self.db.metrics_store.get_tx_gas_data(&tx.hash()).map_err(|e| {
            Error::DatabaseError(format!(
                "[to_tx_record] Failed to fetch the gas data associated with transaction {}: {e:?}",
                tx.hash()
            ))
        })?;

        // Unwrap the option, providing a default value when `None`
        let gas_data = gas_data_option.unwrap_or_else(GasData::default);

        // Process provided block_info option
        let block_info = match block_info_opt {
            // Use provided block_info when present
            Some(block_info) => block_info,
            // Fetch the block info associated with the transaction when block info not provided
            None => {
                match self.get_tx_block_info(&tx.hash())? {
                    Some(block_info) => block_info,
                    // If no associated block info found, throw an error as this should not happen
                    None => {
                        return Err(Error::BlockNotFound(format!(
                            "[to_tx_record] Required `BlockInfo` was not found for transaction: {}",
                            tx.hash()
                        )))
                    }
                }
            }
        };

        // Return transformed transaction record
        Ok(TransactionRecord {
            transaction_hash: tx.hash().to_string(),
            header_hash: block_info.hash().to_string(),
            timestamp: block_info.header.timestamp,
            payload: tx.clone(),
            total_gas_used: gas_data.total_gas_used(),
            wasm_gas_used: gas_data.wasm,
            zk_circuit_gas_used: gas_data.zk_circuits,
            signature_gas_used: gas_data.signatures,
            deployment_gas_used: gas_data.deployments,
        })
    }
}
