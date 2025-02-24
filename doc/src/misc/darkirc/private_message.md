
# Configuring a Private chat between users

Any two users on the `darkirc` server can establish a fully encrypted 
communication medium between each other using a basic keypair setup.

## Configuring darkirc_config.toml

`darkirc_config.toml` should be created by default in `~/.config/darkfi/`
when you first run `darkirc`.

Generate a keypair using the following command: 

```shell
% darkirc --gen-chacha-keypair
```
This will generate a Public Key and a Private Key.

Save the Private key safely & add it to the `darkirc_config.toml` 
file under your contact(s). You may reuse this keypair for multiple
contacts, or generate a new one each time.
```toml
[contact.“satoshi”]
dm_chacha_public = “the_contact_public_key_goes_here”
my_dm_chacha_secret = “your_private_key_for_this_contact_goes_here”
```

To share your Public Key with a user over `darkirc` you can use one of the 
public channels or via an external app like Signal, as plaintext DMs 
are disabled in `darkirc`.

<u><b>Note</b></u>: When sharing/receiving public keys 
(i.e modifying `darkirc_config.toml`), we don't have to restart the 
daemon for the new changes to take effect, we simply send `/rehash`
command from IRC client (or `/quote rehash`)

<u><b>Note</b></u>: If you use the `darkirc`'s public channel, your 
message will be publically visible on the IRC chat.

See the [example darkirc_config.toml](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/bin/darkirc/darkirc_config.toml) for more details

## Example
Lets start by configuring our contacts list in the generated 
`darkirc_config.toml` file (you can also refer to the examples written 
in the comments of the toml file), let's assume Alice and Bob want to
privately chat after they have each other's public keys:

Alice would add bob to her contact list in her own config file:
```toml
[contact.”Bob”]
dm_chacha_public = “D6UzKA6qCG5Mep16i6pJYkUCQcnp46E1jPBsUhyJiXhb”
my_dm_chacha_secret = “A3mLrq4aW9UkFVY4zCfR2aLdEEWVUdH4u8v4o2dgi4kC”
```

And Bob would do the same:
```toml
[contact.”Alice”]
dm_chacha_public = “9sfMEVLphJ4dTX3SEvm6NBhTbWDqfsxu7R2bo88CtV8g”
my_dm_chacha_secret = “E229CzXev335cxhHiJyuzSapz7HMfNzf6ipbginFTvtr”
```

Lets see an Example where 'Alice' sends “Hi” message to 'Bob' using 
the /msg command
```
/msg Bob Hi
```

<u>Note for Weechat Client Users:</u>\
When you private message someone as shown above, the buffer will not 
pop in weechat client until you receive a reply from that person.

For example here 'Alice' will not see any new buffer on her irc interface for 
the recent message which she just send to 'Bob' until 'Bob' replies,
but 'Bob' will get a buffer shown on his irc client with the message 'Hi'.

Reply from 'Bob' to 'Alice'
```
/msg alice welcome!
```

Or instead of `/msg` command, you can use:
```
/query Bob hello
```
This works exactly the same as `/msg` except it will open a new buffer 
with Bob in your client regardless of sending a msg or not.

<u><b>Note</b></u>: The contact name is not the irc nickname, it can 
be anything you want, and you should use it when DMing.

<u><b>Note</b></u>: It's always a good idea to save your keys somewhere safe, but in 
case you lost your Public Key and you still have your Private key in 
`darkirc_config.toml` file, you recover the Public Key like so:
```shell
% darkirc --get_chacha_pubkey <chacha-secret>
```

