pub struct ContractDeploy {
    /// Public address of the contract, derived from the deploy key.
    pub address: pallas::Base,
    /// Public key of the contract, derived from the deploy key.
    /// Used for signatures and authorizations, as well as deriving the
    /// contract's address.
    pub public: PublicKey,
    /// Compiled smart contract wasm binary to be executed in the wasm vm runtime.
    pub wasm_binary: Vec<u8>,
    /// Compiled zkas circuits used by the smart contract provers and verifiers.
    pub circuits: Vec<Vec<u8>>, // XXX: TODO: FIXME: The namespace of the zkas circuit should be in the bin
}
