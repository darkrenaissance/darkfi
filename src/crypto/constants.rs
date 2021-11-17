pub mod fixed_bases;
pub mod sinsemilla;
pub mod util;

pub use fixed_bases::OrchardFixedBases;

pub const DRK_SCHNORR_DOMAIN: &[u8] = b"DarkFi_Schnorr";

pub const MERKLE_DEPTH_ORCHARD: usize = 32;

pub const L_ORCHARD_MERKLE: usize = 255;

