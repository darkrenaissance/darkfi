/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

// TODO: Protocal functions need to be protected so peers can't spam us.

/// Block proposal broadcast protocol
mod protocol_proposal;
pub use protocol_proposal::{ProposalMessage, ProtocolProposal};

/// Validator blockchain sync protocol
mod protocol_sync;
pub use protocol_sync::{
    ForkSyncRequest, ForkSyncResponse, HeaderSyncRequest, HeaderSyncResponse, ProtocolSync,
    SyncRequest, SyncResponse, TipRequest, TipResponse, BATCH,
};

/// Transaction broadcast protocol
mod protocol_tx;
pub use protocol_tx::ProtocolTx;
