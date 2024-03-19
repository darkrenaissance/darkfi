use crate::{
    ethereum::{
        swap_creator::{Swap, SwapCreator},
        Error,
    },
    protocol::traits::{HandleCounterpartyKeysReceivedResult, InitiateSwapArgs, Initiator},
};

use darkfi_serial::async_trait;
use ethers::prelude::*;

use log::info;
use std::sync::Arc;

/// Implemented on top of the non-initiating chain
///
/// Can probably become an extension trait of an RPC client eventually
pub(crate) trait OtherChainClient {
    fn claim_funds(
        &self,
        our_secret: [u8; 32],
        counterparty_secret: [u8; 32],
    ) -> Result<(), crate::Error>;
}

pub(crate) struct EthInitiator<M: Middleware, C: OtherChainClient> {
    contract: SwapCreator<M>,
    middleware: Arc<M>,
    other_chain_client: C,
    secret: [u8; 32],
}

#[allow(dead_code)]
impl<M: Middleware, C: OtherChainClient> EthInitiator<M, C> {
    pub(crate) fn new(
        contract: SwapCreator<M>,
        middleware: Arc<M>,
        other_chain_client: C,
        secret: [u8; 32],
    ) -> Self {
        Self { contract, middleware, other_chain_client, secret }
    }
}

#[async_trait]
impl<M: Middleware + 'static, C: OtherChainClient + Send + Sync> Initiator for EthInitiator<M, C> {
    async fn handle_counterparty_keys_received(
        &self,
        args: InitiateSwapArgs,
    ) -> Result<HandleCounterpartyKeysReceivedResult, crate::Error> {
        use ethers::abi::ParamType;

        let InitiateSwapArgs {
            claim_commitment,
            refund_commitment,
            claimer,
            timeout_duration_1,
            timeout_duration_2,
            asset,
            value,
            nonce,
        } = args;

        // TODO: ERC20 is *not* handled right now
        if asset != Address::zero() {
            return Err(Error::ERC20NotSupported.into());
        }

        let tx = self
            .contract
            .new_swap(
                claim_commitment,
                refund_commitment,
                claimer,
                timeout_duration_1,
                timeout_duration_2,
                asset,
                value,
                nonce,
            )
            .value(value);
        let receipt = tx
            .send()
            .await
            .map_err(|e| Error::FailedToSubmitTransaction("new_swap".to_string(), e.to_string()))?
            .await
            .map_err(|e| Error::FailedToAwaitPendingTransaction("new_swap".to_string(), e))?
            .ok_or_else(|| Error::NoReceipt)?;

        if receipt.status != Some(U64::from(1)) {
            return Err(Error::TransactionFailed("new_swap".to_string(), receipt).into());
        }

        if receipt.logs.len() != 1 {
            return Err(Error::NewSwapUnexpectedLogCount(receipt.logs.len()).into());
        }

        if receipt.logs[0].topics.len() != 1 {
            return Err(Error::NewSwapUnexpectedTopicCount(receipt.logs[0].topics.len()).into());
        }

        let log_data = &receipt.logs[0].data;

        // ABI-unpack log data
        // note: there are other parameters emitted in the log, but we don't care about them
        let mut tokens = ethers::abi::decode(
            &vec![
                ParamType::FixedBytes(32),
                ParamType::FixedBytes(32),
                ParamType::FixedBytes(32),
                ParamType::Uint(256),
                ParamType::Uint(256),
            ],
            &log_data.0,
        )
        .map_err(|e| Error::NewSwapLogDecodingFailed(e))?;

        if tokens.len() != 5 {
            return Err(Error::NewSwapUnexpectedLogTokenCount(tokens.len()).into());
        }

        let swap_id = match tokens.remove(0) {
            ethers::abi::Token::FixedBytes(bytes) => {
                // this shouldn't happen, would be an error in ethers-rs
                if bytes.len() != 32 {
                    return Err(Error::FixedBytesDecodingError(bytes.len()).into());
                }

                let mut swap_id = [0u8; 32];
                swap_id.copy_from_slice(&bytes);
                swap_id
            }
            token => {
                return Err(Error::ExpectedFixedBytes(token).into());
            }
        };
        let (timeout_1, timeout_2) = match (tokens.remove(2), tokens.remove(2)) {
            // tokens index 3 and 4
            (ethers::abi::Token::Uint(timeout_1), ethers::abi::Token::Uint(timeout_2)) => {
                (timeout_1, timeout_2)
            }
            _ => {
                return Err(Error::ExpectedTwoU256s.into());
            }
        };

        let contract_swap = Swap {
            owner: self.middleware.default_sender().expect("must have a default sender"),
            claim_commitment,
            refund_commitment,
            claimer,
            timeout_1,
            timeout_2,
            asset,
            value,
            nonce,
        };

        info!(
            "initiated swap on-chain: contract_swap_id = {}",
            ethers::utils::hex::encode(&swap_id)
        );

        Ok(HandleCounterpartyKeysReceivedResult {
            contract_swap_id: swap_id,
            contract_swap,
            block_number: receipt
                .block_number
                .expect("block number must be set in receipt")
                .as_u64(),
        })
    }

    async fn handle_counterparty_funds_locked(
        &self,
        swap: super::swap_creator::Swap,
        swap_id: [u8; 32],
    ) -> Result<(), crate::Error> {
        let tx = self.contract.set_ready(swap);
        let receipt = tx
            .send()
            .await
            .map_err(|e| Error::FailedToSubmitTransaction("set_ready".to_string(), e.to_string()))?
            .await
            .map_err(|e| Error::FailedToAwaitPendingTransaction("set_ready".to_string(), e))?
            .ok_or_else(|| Error::NoReceipt)?;

        if receipt.status != Some(U64::from(1)) {
            return Err(Error::TransactionFailed("set_ready".to_string(), receipt).into());
        }

        info!("contract set to ready, contract_swap_id = {}", ethers::utils::hex::encode(swap_id));
        Ok(())
    }

    async fn handle_counterparty_funds_claimed(
        &self,
        counterparty_secret: [u8; 32],
    ) -> Result<(), crate::Error> {
        self.other_chain_client.claim_funds(self.secret, counterparty_secret)
    }

    async fn handle_should_refund(
        &self,
        swap: super::swap_creator::Swap,
    ) -> Result<(), crate::Error> {
        let tx = self.contract.refund(swap, self.secret);

        let receipt = tx
            .send()
            .await
            .map_err(|e| Error::FailedToSubmitTransaction("refund".to_string(), e.to_string()))?
            .await
            .map_err(|e| Error::FailedToAwaitPendingTransaction("refund".to_string(), e))?
            .ok_or_else(|| Error::NoReceipt)?;

        if receipt.status != Some(U64::from(1)) {
            return Err(Error::TransactionFailed("refund".to_string(), receipt).into());
        }

        Ok(())
    }
}
