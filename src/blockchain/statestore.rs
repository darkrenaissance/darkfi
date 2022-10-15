use darkfi_serial::serialize;

use crate::{crypto::contract_id::ContractId, Result};

const SLED_STATES_TREE: &[u8] = b"_states";

/// The `StateStore` is a `sled` tree storing states of deployed contracts.
/// The states themselves are data that is allocated and stored as raw bytes.
/// These bytes are (de)serialized by the code in wasm and the contracts can
/// operate on the state data themselves. Regarding on the (byte) size of the
/// state, the contract deployer should allocate and pay for a certain size of
/// their state stored by all the nodes. The cost should be linear to the byte
/// size used.
#[derive(Clone)]
pub struct StateStore(sled::Tree);

impl StateStore {
    /// Opens a new or existing `StateStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_STATES_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a state into the store. This will replace the previous state.
    /// The contract's ID is used as a key, while the value is the contract
    /// state serialized to bytes.
    pub fn insert(&self, contract_id: &ContractId, contract_state: &[u8]) -> Result<()> {
        self.0.insert(serialize(contract_id), contract_state.to_vec())?;
        Ok(())
    }

    /// Check if the `StateStore` contains a state for the given `ContractId`.
    pub fn contains(&self, contract_id: &ContractId) -> Result<bool> {
        Ok(self.0.contains_key(serialize(contract_id))?)
    }

    /// Retrieve a state from the `StateStore` given a `ContractId` if it exists.
    pub fn get(&self, contract_id: &ContractId) -> Result<Option<Vec<u8>>> {
        if let Some(data) = self.0.get(serialize(contract_id))? {
            return Ok(Some(data.to_vec()))
        }

        Ok(None)
    }
}
