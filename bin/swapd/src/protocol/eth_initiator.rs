use crate::protocol::{
    swap_creator::{Swap, SwapCreator},
    traits::{HandleCounterpartyKeysReceivedResult, InitiateSwapArgs, Initiator},
};
use darkfi_serial::async_trait;
use ethers::prelude::*;
use eyre::{ensure, Result, WrapErr as _};

/// Implemented on top of the non-initiating chain
pub(crate) trait OtherChainClient {
    fn claim_funds(&self, our_secret: [u8; 32], counterparty_secret: [u8; 32]) -> Result<()>;
}

pub(crate) struct EthInitiator<M: Middleware, C: OtherChainClient> {
    contract: SwapCreator<M>,
    other_chain_client: C,
    secret: [u8; 32],
}

#[async_trait]
impl<M: Middleware + 'static, C: OtherChainClient + Send + Sync> Initiator for EthInitiator<M, C> {
    async fn handle_counterparty_keys_received(
        &self,
        args: InitiateSwapArgs,
    ) -> Result<HandleCounterpartyKeysReceivedResult> {
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
        ensure!(asset == Address::zero(), "ERC20 not supported yet");

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
            .wrap_err("failed to submit transaction")?
            .await
            .wrap_err("failed to await pending transaction")?
            .ok_or_else(|| eyre::eyre!("no receipt?"))?;

        ensure!(
            receipt.status == Some(U64::from(1)),
            "`newSwap` transaction failed: {:?}",
            receipt
        );
        ensure!(receipt.logs.len() == 1, "expected exactly one log, got {:?}", receipt.logs);
        ensure!(
            receipt.logs[0].topics.len() == 1,
            "expected exactly one topic, got {:?}",
            receipt.logs[0].topics
        );
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
        )?;

        ensure!(tokens.len() == 5, "expected five tokens, got {:?}", tokens.len());

        let swap_id = match tokens.remove(0) {
            ethers::abi::Token::FixedBytes(bytes) => {
                // this shouldn't happen, would be an error in ethers-rs
                ensure!(bytes.len() == 32, "expected exactly 32 bytes, got {:?}", bytes.len());

                let mut swap_id = [0u8; 32];
                swap_id.copy_from_slice(&bytes);
                swap_id
            }
            _ => {
                return Err(eyre::eyre!("expected FixedBytes, got something else"));
            }
        };
        let (timeout_1, timeout_2) = match (tokens.remove(3), tokens.remove(4)) {
            (ethers::abi::Token::Uint(timeout_1), ethers::abi::Token::Uint(timeout_2)) => {
                (timeout_1, timeout_2)
            }
            _ => {
                return Err(eyre::eyre!("expected two U256s, got something else"));
            }
        };

        let contract_swap = Swap {
            owner: Address::zero(), // TODO
            claim_commitment,
            refund_commitment,
            claimer,
            timeout_1,
            timeout_2,
            asset,
            value,
            nonce,
        };

        Ok(HandleCounterpartyKeysReceivedResult { contract_swap_id: swap_id, contract_swap })
    }

    async fn handle_counterparty_funds_locked(
        &self,
        swap: super::swap_creator::Swap,
    ) -> Result<()> {
        let tx = self.contract.set_ready(swap);
        let receipt = tx
            .send()
            .await
            .wrap_err("failed to submit transaction")?
            .await
            .wrap_err("failed to await pending transaction")?
            .ok_or_else(|| eyre::eyre!("no receipt?"))?;

        ensure!(
            receipt.status == Some(U64::from(1)),
            "`setReady` transaction failed: {:?}",
            receipt
        );

        Ok(())
    }

    async fn handle_counterparty_funds_claimed(&self, counterparty_secret: [u8; 32]) -> Result<()> {
        self.other_chain_client.claim_funds(self.secret, counterparty_secret)
    }

    async fn handle_should_refund(&self, swap: super::swap_creator::Swap) -> Result<()> {
        let tx = self.contract.refund(swap, self.secret);

        let receipt = tx
            .send()
            .await
            .wrap_err("failed to submit transaction")?
            .await
            .wrap_err("failed to await pending transaction")?
            .ok_or_else(|| eyre::eyre!("no receipt?"))?;

        ensure!(receipt.status == Some(U64::from(1)), "`refund` transaction failed: {:?}", receipt);

        Ok(())
    }
}
