use crate::protocol::traits::Initiator;

pub(crate) struct EthInitiator {
    
}

impl Initiator for EthInitiator {
    fn handle_counterparty_keys_received(&self) {
        // ...
    }

    fn handle_counterparty_funds_locked(&self) {
        // ...
    }

    fn handle_counterparty_funds_claimed(&self) {
        // ...
    }

    fn handle_should_refund(&self) {
        // ...
    }
}
