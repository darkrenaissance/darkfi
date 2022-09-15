/// Base field arithmetic gadget
pub mod arithmetic;


/// Small range check, 0..8 bits
pub mod small_range_check;

/// Field-native range check gadget with a lookup table
pub mod native_range_check;

/// Field-native less than comparison gadget with a lookup table
pub mod less_than;

/// is_zero comparison gadget
pub mod is_zero;
