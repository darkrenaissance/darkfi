
// These are the different sled trees that will be created
pub const ZKAS_TREE: &str = "zkas"; // <-- This should be a constant in darkfi-sdk
pub const COIN_ROOTS_TREE: &str = "coin_roots";
pub const NULLIFIERS_TREE: &str = "nullifiers";
pub const INFO_TREE: &str = "info";

// These are the keys inside the info tree
pub const COIN_MERKLE_TREE: &str = "coin_tree";
pub const FAUCET_PUBKEYS: &str = "faucet_pubkeys";

/// zkas mint contract namespace
pub const ZKAS_MINT_NS: &str = "Mint";
/// zkas burn contract namespace
pub const ZKAS_BURN_NS: &str = "Burn";