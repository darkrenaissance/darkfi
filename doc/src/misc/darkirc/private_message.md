# Configure encrypted direct messages

DarkIRC disables plaintext direct messages. Two users can configure a
`crypto_box::ChaChaBox` using each other's public key and their own secret key.
The resulting Event Graph message fields are encrypted and base58 encoded.

This is static public-key encryption, not the Signal protocol: it has no
ratchet, automatic key verification, or forward secrecy. Protect the secret
keys and authenticate public keys through a trusted channel.

## Generate and exchange keys

Each participant generates a keypair locally:

```shell
% darkirc --gen-chacha-keypair
```

Share only the generated public key. A public DarkIRC channel is visible to
everyone and retained by nodes, so use an authenticated out-of-band channel
when key substitution is a concern. Never share the secret key.

If a secret key remains available but its public key was lost, derive it with:

```shell
% darkirc --get-chacha-pubkey BASE58_SECRET_KEY
```

## Configure both participants

Suppose Alice labels Bob as `Bob`. Alice places Bob's public key and Alice's
secret key in her configuration:

```toml
[contact."Bob"]
dm_chacha_public = "BOBS_BASE58_PUBLIC_KEY"
my_dm_chacha_secret = "ALICES_BASE58_SECRET_KEY"
```

Bob configures the inverse relationship:

```toml
[contact."Alice"]
dm_chacha_public = "ALICES_BASE58_PUBLIC_KEY"
my_dm_chacha_secret = "BOBS_BASE58_SECRET_KEY"
```

The contact label is local, case-sensitive, and does not have to equal the
other user's current IRC nickname. Use the exact configured label when
sending a message.

After editing `[contact.*]`, reload contacts from the IRC client:

```text
/rehash
```

You can also restart DarkIRC. Invalid base58 or a key that is not 32 bytes is
rejected while loading the configuration.

## Send messages

Alice can send to her configured `Bob` contact with either command:

```text
/msg Bob Hi
/query Bob Hi
```

`/query` commonly opens the contact buffer immediately; `/msg` buffer behavior
depends on the IRC client. A direct message to a name absent from the local
`[contact.*]` table is refused rather than sent as plaintext.

Reusing one local keypair for several contacts is supported, but separate
keypairs reduce the impact and linkability of one compromised key. Back up the
configuration securely if the keys must survive a datastore or device loss.
