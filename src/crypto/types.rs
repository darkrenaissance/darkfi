//! Type aliases used in the codebase.
// Helpful for changing the curve and crypto we're using.
use pasta_curves::pallas;

pub type DrkCircuitField = pallas::Base;

pub type DrkTokenId = pallas::Base;
pub type DrkValue = pallas::Base;
pub type DrkSerial = pallas::Base;

pub type DrkSpendHook = pallas::Base;
pub type DrkUserData = pallas::Base;
pub type DrkUserDataBlind = pallas::Base;
pub type DrkUserDataEnc = pallas::Base;

pub type DrkCoinBlind = pallas::Base;
pub type DrkValueBlind = pallas::Scalar;
pub type DrkValueCommit = pallas::Point;
