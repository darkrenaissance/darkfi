pub mod bridge;
pub mod gateway;
pub mod reqrep;

#[cfg(feature = "btc")]
pub mod btc;
#[cfg(feature = "btc")]
pub use btc::{Account, BtcFailed, BtcResult, Keypair, PubAddress};

#[cfg(feature = "sol")]
pub mod sol;
#[cfg(feature = "sol")]
pub use sol::{SolClient, SolFailed, SolResult};

#[cfg(feature = "eth")]
pub mod eth;
#[cfg(feature = "eth")]
pub use eth::{EthClient, EthFailed, EthResult};

pub use gateway::{GatewayClient, GatewayService, GatewaySlabsSubscriber};
