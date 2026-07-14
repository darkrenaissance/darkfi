# DarkIRC protocol and data reference

This page describes the current DarkIRC-specific data placed in the Event
Graph and the IRC surface exposed to local clients. The underlying peer
protocol is documented in the [Event Graph network protocol](../event_graph/network_protocol.md).

## Event content

Each chat event serializes the following `Privmsg` structure:

| Field | Type | Current meaning |
| --- | --- | --- |
| `version` | `u8` | Format version; newly emitted messages currently use `0`. |
| `msg_type` | `u8` | Message subtype; newly emitted messages currently use `0`. |
| `channel` | `String` | Channel name, or an encrypted dummy value for a DM. |
| `nick` | `String` | Sender nickname, or an encrypted dummy value for a DM. |
| `msg` | `String` | Message body or encoded ciphertext. |

Nicknames and channel names are limited to 24 bytes. Message bodies and topics
are limited to 512 bytes. The IRC input buffer is limited to 1024 bytes.

### Public channels

Without a configured channel secret, `channel`, `nick`, and `msg` are stored as
plaintext. Public channel history should be treated as public and persistent.

### Encrypted channels

For a `[channel."#name"]` with a `secret`, DarkIRC constructs a
`crypto_box::ChaChaBox` from that 32-byte base58 secret. The channel name,
padded nickname, and message are encrypted independently and encoded with
base58. Every participant must configure the identical secret.

```toml
[channel."#project"]
secret = "BASE58_32_BYTE_SECRET"
topic = "Private project channel"
```

Generate a secret with `darkirc --gen-channel-secret`.

### Direct messages

For a configured contact, DarkIRC encrypts dummy channel and nickname fields
and encrypts the message with the contact ChaCha box. Contact configuration
requires the contact's public key and the local secret key:

```toml
[contact."Bob"]
dm_chacha_public = "BOBS_BASE58_PUBLIC_KEY"
my_dm_chacha_secret = "ALICES_BASE58_SECRET_KEY"
```

See [encrypted direct messages](private_message.md) for the two-party setup and
security limitations.

## Local IRC interface

DarkIRC currently handles these IRC commands:

`ADMIN`, `CAP`, `INFO`, `JOIN`, `LIST`, `MODE`, `MOTD`, `NAMES`, `NICK`,
`PART`, `PASS`, `PING`, `PRIVMSG`, `REHASH`, `TOPIC`, `USER`, and `VERSION`.

It provides the custom client capabilities `no-history` and `no-autojoin`.
NickServ commands are sent with `PRIVMSG NickServ ...` when RLN is enabled.
DarkIRC does not claim complete RFC 2812 compatibility; commands that depend
on conventional centralized IRC server state may be absent or have P2P-specific
semantics.

## Reloadable configuration

`REHASH` reloads `autojoin`, `[channel.*]`, and `[contact.*]` from the active
configuration file. P2P, datastore, history, archive, RPC, listener, password,
and RLN settings require a daemon restart.
