/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use darkfi::blockchain::Header;
use darkfi_serial::serialize;

use crate::proto::{
    ForkHeaderHashRequest, ForkHeaderHashResponse, ForkHeadersRequest, ForkHeadersResponse,
    ForkProposalsRequest, ForkSyncRequest, HeaderSyncRequest, HeaderSyncResponse, SyncRequest,
    TipRequest, TipResponse, BATCH,
};

#[test]
fn darkfid_protocols_metering() {
    // Known constant bytes lengths
    const BOOL_LEN: usize = 1;
    const OPTION_LEN: usize = 1;
    const U32_LEN: usize = 4;
    const VARINT_LEN: usize = 1;
    const HEADER_HASH_LEN: usize = 32;
    // Header = U8_LEN + HEADER_HASH_LEN + U32_LEN + U64_LEN + U64_LEN + (U8_LEN * 32) =
    // 1 + 32 + 4 + 8 + 8 + (1 * 32) = 53 + 32 = 85
    const HEADER_LEN: usize = 85;

    // Generate a dummy `Header`.
    // Its bytes vector length is constant.
    let header = Header::default();
    assert_eq!(serialize(&header).len(), HEADER_LEN);

    // Its hash bytes vector length is constant.
    let header_hash = header.hash();
    assert_eq!(serialize(&header_hash).len(), HEADER_HASH_LEN);

    // Protocol sync `TipRequest` message has constant bytes length
    let tip_request = TipRequest { tip: header_hash };
    assert_eq!(serialize(&tip_request).len(), HEADER_HASH_LEN);

    // Protocol sync `TipResponse` message has constant bytes length,
    // based on its structure.
    let tip_response = TipResponse { synced: false, height: None, hash: None };
    // Length = BOOL_LEN + OPTION_LEN + OPTION_LEN = 1 + 1 + 1 = 3
    assert_eq!(serialize(&tip_response).len(), BOOL_LEN + OPTION_LEN + OPTION_LEN);
    let tip_response = TipResponse { synced: false, height: Some(42), hash: None };
    // Length = BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN = 1 + 1 + 4 + 1 = 7
    assert_eq!(serialize(&tip_response).len(), BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN);
    let tip_response = TipResponse { synced: false, height: None, hash: Some(header_hash) };
    // Length = BOOL_LEN + OPTION_LEN + OPTION_LEN + HEADER_HASH_LEN = 1 + 1 + 1 + 32 = 35
    assert_eq!(
        serialize(&tip_response).len(),
        BOOL_LEN + OPTION_LEN + OPTION_LEN + HEADER_HASH_LEN
    );
    let tip_response = TipResponse { synced: false, height: Some(42), hash: Some(header_hash) };
    // Length = BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN + HEADER_HASH_LEN =
    // 1 + 1 + 4 + 1 + 32 = 39
    assert_eq!(
        serialize(&tip_response).len(),
        BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN + HEADER_HASH_LEN
    );
    let tip_response = TipResponse { synced: true, height: None, hash: None };
    // Length = BOOL_LEN + OPTION_LEN + OPTION_LEN = 1 + 1 + 1 = 3
    assert_eq!(serialize(&tip_response).len(), BOOL_LEN + OPTION_LEN + OPTION_LEN);
    let tip_response = TipResponse { synced: true, height: Some(42), hash: None };
    // Length = BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN = 1 + 1 + 4 + 1 = 7
    assert_eq!(serialize(&tip_response).len(), BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN);
    let tip_response = TipResponse { synced: true, height: None, hash: Some(header_hash) };
    // Length = BOOL_LEN + OPTION_LEN + OPTION_LEN + HEADER_HASH_LEN = 1 + 1 + 1 + 32 = 35
    assert_eq!(
        serialize(&tip_response).len(),
        BOOL_LEN + OPTION_LEN + OPTION_LEN + HEADER_HASH_LEN
    );
    let tip_response = TipResponse { synced: true, height: Some(42), hash: Some(header_hash) };
    // Length = BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN + HEADER_HASH_LEN =
    // 1 + 1 + 4 + 1 + 32 = 39
    assert_eq!(
        serialize(&tip_response).len(),
        BOOL_LEN + OPTION_LEN + U32_LEN + OPTION_LEN + HEADER_HASH_LEN
    );

    // Protocol sync `HeaderSyncRequest` message has constant bytes length
    let header_sync_request = HeaderSyncRequest { height: 42 };
    // Length = 4
    assert_eq!(serialize(&header_sync_request).len(), U32_LEN);

    // Protocol sync `HeaderSyncResponse` is limited by `BATCH` so it has a
    // constant max bytes length limit.
    let header_sync_response = HeaderSyncResponse { headers: vec![header.clone(); BATCH] };
    // When we serialize a `Vec`, its length is encoded as a `VarInt`.
    // Based on length size/type, this can add from 1(u8) to 8(u64) bytes.
    // Since `BATCH` is 20, its `VarInt` will be represented as a u8,
    // adding an extra byte.
    // Length = (BATCH * HEADER_LEN) + VARINT_LEN = (20 * 85) + 1 = 1700 + 1 = 1701
    assert_eq!(serialize(&header_sync_response).len(), (BATCH * HEADER_LEN) + VARINT_LEN);

    // Protocol sync `SyncRequest` is limited by `BATCH` so it has a
    // constant max bytes length limit.
    let sync_request = SyncRequest { headers: vec![header_hash; BATCH] };
    // Don't forget the extra byte from `Vec` length.
    // Length = (BATCH * HEADER_HASH_LEN) + VARINT_LEN = (20 * 32) + 1 = 640 + 1 = 641
    assert_eq!(serialize(&sync_request).len(), (BATCH * HEADER_HASH_LEN) + VARINT_LEN);

    // Protocol sync `SyncResponse` is limited by `BATCH` so it can have a
    // constant max bytes length limit, but we are not limiting `BlockInfo` size.

    // Protocol sync `ForkSyncRequest` message has constant bytes length,
    // based on its structure.
    let fork_sync_request = ForkSyncRequest { tip: header_hash, fork_tip: None };
    // Length = HEADER_HASH_LEN + OPTION_LEN = 32 + 1 = 33
    assert_eq!(serialize(&fork_sync_request).len(), HEADER_HASH_LEN + OPTION_LEN);
    let fork_sync_request = ForkSyncRequest { tip: header_hash, fork_tip: Some(header_hash) };
    // Length = HEADER_HASH_LEN + OPTION_LEN + HEADER_HASH_LEN = 32 + 1 + 32 = 65
    assert_eq!(serialize(&fork_sync_request).len(), HEADER_HASH_LEN + OPTION_LEN + HEADER_HASH_LEN);

    // Protocol sync `ForkSyncResponse` is limited by `BATCH` so it can have a
    // constant max bytes length limit, but we are not limiting `Proposal` size.

    // Protocol sync `ForkHeaderHashRequest` message has constant bytes length
    let fork_header_hash_request = ForkHeaderHashRequest { height: 42, fork_header: header_hash };
    // Length = U32_LEN + HEADER_HASH_LEN = 4 + 32 = 36
    assert_eq!(serialize(&fork_header_hash_request).len(), U32_LEN + HEADER_HASH_LEN);

    // Protocol sync `ForkHeaderHashResponse` message has constant bytes length,
    // based on its structure.
    let fork_header_hash_response = ForkHeaderHashResponse { fork_header: None };
    // Length = OPTION_LEN = 1
    assert_eq!(serialize(&fork_header_hash_response).len(), OPTION_LEN);
    let fork_header_hash_response = ForkHeaderHashResponse { fork_header: Some(header_hash) };
    // Length = OPTION_LEN + HEADER_HASH_LEN = 1 + 32 = 33
    assert_eq!(serialize(&fork_header_hash_response).len(), OPTION_LEN + HEADER_HASH_LEN);

    // Protocol sync `ForkHeadersRequest` is limited by `BATCH` so it has a
    // constant max bytes length limit.
    let fork_headers_request =
        ForkHeadersRequest { headers: vec![header_hash; BATCH], fork_header: header_hash };
    // Don't forget the extra byte from `Vec` length.
    // Length = (BATCH * HEADER_HASH_LEN) + VARINT_LEN + HEADER_HASH_LEN =
    // (20 * 32) + 1 + 32 = 640 + 33 = 673
    assert_eq!(
        serialize(&fork_headers_request).len(),
        (BATCH * HEADER_HASH_LEN) + VARINT_LEN + HEADER_HASH_LEN
    );

    // Protocol sync `ForkHeadersResponse` is limited by `BATCH` so it has a
    // constant max bytes length limit.
    let fork_headers_response = ForkHeadersResponse { headers: vec![header; BATCH] };
    // Don't forget the extra byte from `Vec` length.
    // Length = (BATCH * HEADER_LEN) + VARINT_LEN = (20 * 85) + 1 = 1700 + 1 = 1701
    assert_eq!(serialize(&fork_headers_response).len(), (BATCH * HEADER_LEN) + VARINT_LEN);

    // Protocol sync `ForkProposalsRequest` is limited by `BATCH` so it has a
    // constant max bytes length limit.
    let fork_proposals_request =
        ForkProposalsRequest { headers: vec![header_hash; BATCH], fork_header: header_hash };
    // Don't forget the extra byte from `Vec` length.
    // Length = (BATCH * HEADER_HASH_LEN) + VARINT_LEN + HEADER_HASH_LEN =
    // (20 * 32) + 1 + 32 = 640 + 33 = 673
    assert_eq!(
        serialize(&fork_proposals_request).len(),
        (BATCH * HEADER_HASH_LEN) + VARINT_LEN + HEADER_HASH_LEN
    );

    // Protocol sync `ForkProposalsResponse` is limited by `BATCH` so it can have a
    // constant max bytes length limit, but we are not limiting `Proposal` size.

    // Protocol proposal `ProposalMessage` can have a constant max bytes length limit,
    // but we are not limiting `Proposal` size.

    // Protocol tx `Transaction` can have constant max bytes length limit,
    // but we are not limiting `Transaction` size.
}
