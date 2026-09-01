/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! The chatview storage wire format.
//!
//! kv key:   `[u64 BE ts][32-byte msg id]`
//! kv value: `[u8 tag][type-owned bytes]` — the tag is the `MsgType`
//! discriminant, the rest is owned by the message type.
//!
//! privmsg payload: `[nick: String][text: String][confirmed: bool]`
//! datemsg payload: `[local-midnight ts: u64]` (derived; never stored)
//!
//! This is a clean break from the previous chatview: values it cannot
//! decode are corrupt data and fail explicitly (panic) with a message
//! identifying the entry — never a silent skip or a misread.

use std::io::Cursor;

use darkfi_serial::{Decodable, Encodable};

use super::{MessageId, MsgRecord, MsgType, Timestamp};

/// Serialized kv key length: 8-byte timestamp + 32-byte msg id.
pub const KEY_LEN: usize = 8 + 32;

/// Decode a wire tag. Unknown tags are corrupt data and panic — errors
/// are always explicit, never a silent skip.
pub fn msg_type_from_u8(tag: u8) -> MsgType {
    match tag {
        0 => MsgType::PrivMsg,
        1 => MsgType::FileMsg,
        2 => MsgType::DateMsg,
        _ => panic!("unknown msg type tag {tag}"),
    }
}

/// Encode an entry key: `[u64 BE ts][msg id]`.
pub fn encode_key(ts: Timestamp, id: &MessageId) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    key[..8].copy_from_slice(&ts.to_be_bytes());
    key[8..].copy_from_slice(&id.0);
    key
}

/// Decode an entry key back into its `(ts, msg_id)` composite.
///
/// ## Panics
///
/// If the key is not [`KEY_LEN`] bytes.
pub fn decode_key(key: &[u8]) -> (Timestamp, MessageId) {
    assert_eq!(key.len(), KEY_LEN, "corrupt chat entry key: {key:?} (expected {KEY_LEN} bytes)");
    let ts_bytes: [u8; 8] = key[..8].try_into().unwrap();
    let id: [u8; 32] = key[8..].try_into().unwrap();
    (Timestamp::from_be_bytes(ts_bytes), MessageId(id))
}

/// Encode an entry value: `[tag][type-owned payload]`.
pub fn encode_value(msg_type: MsgType, payload: &[u8]) -> Vec<u8> {
    let mut val = Vec::with_capacity(1 + payload.len());
    val.push(msg_type as u8);
    val.extend_from_slice(payload);
    val
}

/// Decode an entry value into a record (height starts at zero; the
/// owning type node measures once materialized).
///
/// ## Panics
///
/// On an empty value, an unknown type tag, or an undecodable payload —
/// identifying the entry by ts/id and the failure. Corrupt data is
/// never silently skipped.
pub fn decode_value(val: &[u8], ts: Timestamp, id: &MessageId) -> MsgRecord {
    let Some((&tag, payload)) = val.split_first() else {
        panic!("corrupt chat entry: empty value [ts={ts} id={id}]")
    };
    let msg_type = msg_type_from_u8(tag);

    match msg_type {
        MsgType::PrivMsg => {
            let (_nick, _text, _confirmed) = decode_privmsg_payload(payload, ts, id);
        }
        MsgType::DateMsg => {
            let _midnight = decode_datemsg_payload(payload, ts, id);
        }
        MsgType::FileMsg => {
            panic!("corrupt chat entry: filemsg is derived and never stored [ts={ts} id={id}]")
        }
    }

    MsgRecord { ts, id: *id, msg_type, payload: payload.to_vec(), height: 0. }
}

/// Encode the privmsg payload: `[nick][text][confirmed]`.
pub fn encode_privmsg_payload(nick: &str, text: &str, confirmed: bool) -> Vec<u8> {
    let mut payload = vec![];
    nick.encode(&mut payload).unwrap();
    text.encode(&mut payload).unwrap();
    confirmed.encode(&mut payload).unwrap();
    payload
}

/// Decode the privmsg payload back into `(nick, text, confirmed)`.
///
/// ## Panics
///
/// If the payload does not decode, identifying the entry.
pub fn decode_privmsg_payload(
    payload: &[u8],
    ts: Timestamp,
    id: &MessageId,
) -> (String, String, bool) {
    let mut cur = Cursor::new(payload);
    let ctx = |what: &str| format!("corrupt chat entry: {what} [ts={ts} id={id}]");
    let nick =
        String::decode(&mut cur).unwrap_or_else(|e| panic!("{}: {e}", ctx("bad privmsg nick")));
    let text =
        String::decode(&mut cur).unwrap_or_else(|e| panic!("{}: {e}", ctx("bad privmsg text")));
    let confirmed = bool::decode(&mut cur)
        .unwrap_or_else(|e| panic!("{}: {e}", ctx("bad privmsg confirmed flag")));
    (nick, text, confirmed)
}

/// Encode the datemsg payload: the separator day's local-midnight ts.
pub fn encode_datemsg_payload(midnight: Timestamp) -> Vec<u8> {
    let mut payload = vec![];
    midnight.encode(&mut payload).unwrap();
    payload
}

/// Decode the datemsg payload back into its midnight timestamp.
///
/// ## Panics
///
/// If the payload does not decode, identifying the entry.
pub fn decode_datemsg_payload(payload: &[u8], ts: Timestamp, id: &MessageId) -> Timestamp {
    let mut cur = Cursor::new(payload);
    Timestamp::decode(&mut cur).unwrap_or_else(|e| {
        panic!("corrupt chat entry: bad datemsg midnight [ts={ts} id={id}]: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rid(b: u8) -> MessageId {
        MessageId([b; 32])
    }

    #[test]
    fn key_round_trip() {
        let key = encode_key(1_756_000_000_000, &rid(7));
        assert_eq!(key.len(), KEY_LEN);
        assert_eq!(decode_key(&key), (1_756_000_000_000, rid(7)));
        // BE ordering: the ts leads.
        assert_eq!(&key[..8], &1_756_000_000_000u64.to_be_bytes());
    }

    #[test]
    fn privmsg_value_round_trip() {
        let id = rid(1);
        let payload = encode_privmsg_payload("alice", "hello world", false);
        let val = encode_value(MsgType::PrivMsg, &payload);
        assert_eq!(val[0], MsgType::PrivMsg as u8);

        let rec = decode_value(&val, 123, &id);
        assert_eq!(rec.ts, 123);
        assert_eq!(rec.id, id);
        assert_eq!(rec.msg_type, MsgType::PrivMsg);
        assert_eq!(rec.height, 0.);
        assert_eq!(
            decode_privmsg_payload(&rec.payload, 123, &id),
            ("alice".to_string(), "hello world".to_string(), false)
        );

        let payload = encode_privmsg_payload("NOTICE", "\u{1}ACTION waves\u{1}", true);
        let val = encode_value(MsgType::PrivMsg, &payload);
        let rec = decode_value(&val, 124, &id);
        assert_eq!(
            decode_privmsg_payload(&rec.payload, 124, &id),
            ("NOTICE".to_string(), "\u{1}ACTION waves\u{1}".to_string(), true)
        );
    }

    #[test]
    fn datemsg_value_round_trip() {
        let payload = encode_datemsg_payload(1_755_936_000_000);
        let val = encode_value(MsgType::DateMsg, &payload);
        let rec = decode_value(&val, 1_755_936_000_000, &rid(0));
        assert_eq!(rec.msg_type, MsgType::DateMsg);
        assert_eq!(decode_datemsg_payload(&rec.payload, 0, &rid(0)), 1_755_936_000_000);
    }

    #[test]
    fn discriminants_are_the_wire_tags() {
        assert_eq!(MsgType::PrivMsg as u8, 0);
        assert_eq!(MsgType::FileMsg as u8, 1);
        assert_eq!(MsgType::DateMsg as u8, 2);
        assert_eq!(msg_type_from_u8(0), MsgType::PrivMsg);
        assert_eq!(msg_type_from_u8(1), MsgType::FileMsg);
        assert_eq!(msg_type_from_u8(2), MsgType::DateMsg);
    }

    #[test]
    #[should_panic(expected = "unknown msg type tag 9")]
    fn unknown_tag_panics() {
        decode_value(&[9, 0, 0], 1, &rid(1));
    }

    #[test]
    #[should_panic(expected = "empty value")]
    fn empty_value_panics() {
        decode_value(&[], 1, &rid(1));
    }

    #[test]
    #[should_panic(expected = "bad privmsg text")]
    fn truncated_privmsg_payload_panics() {
        let mut payload = encode_privmsg_payload("alice", "hello", true);
        payload.truncate(6);
        decode_privmsg_payload(&payload, 5, &rid(2));
    }

    #[test]
    #[should_panic(expected = "bad privmsg confirmed flag")]
    fn missing_confirmed_flag_panics() {
        let mut payload = encode_privmsg_payload("alice", "hello", true);
        payload.pop();
        decode_privmsg_payload(&payload, 5, &rid(2));
    }

    #[test]
    #[should_panic(expected = "unknown msg type tag")]
    fn legacy_untagged_value_panics() {
        // The old chatview stored `nick, text` with no tag byte; the
        // first byte (the nick's varint length) reads as a tag and must
        // fail loudly, never misread.
        let mut legacy = vec![];
        "alice".encode(&mut legacy).unwrap();
        "hello".encode(&mut legacy).unwrap();
        decode_value(&legacy, 1, &rid(1));
    }

    #[test]
    #[should_panic(expected = "filemsg is derived and never stored")]
    fn stored_filemsg_panics() {
        decode_value(&[1, 0, 0], 1, &rid(1));
    }

    #[test]
    #[should_panic(expected = "expected 40 bytes")]
    fn bad_key_length_panics() {
        decode_key(&[0u8; 12]);
    }
}
