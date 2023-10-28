
# Configuring a Private chat between users

Any two users on the `ircd` server can establish a fully encrypted 
communication medium between each other using a basic keypair setup.

## Configuring ircd_config.toml

`ircd_config.toml` should be created by default in `~/.config/darkfi/`
when you first run `ircd`.

Generate a keypair using the following command: 

```shell
% ircd --gen-keypair
```
This will generate a Public Key and a Private Key.

Save the Private key safely & add it to the `ircd_config.toml` file as shown below.

```toml
[private_key.”your_private_key_goes_here”]
```

To share your Public Key with a user over `ircd` you can use one of the 
public channels or via an external app like Signal, as plaintext DMs 
are disabled in `ircd`.

<u><b>Note</b></u>: If you use the `ircd`'s public channel, your 
message will be publically visible on the IRC chat.

See the [example ircd_config.toml](https://github.com/darkrenaissance/darkfi/blob/v0.4.1/bin/ircd/ircd_config.toml) for more details

## Example
Lets start by configuring our contacts list in the generated 
`ircd_config.toml` file (you can also refer to the examples written 
in the comments of the toml file), let's assume alice and bob want to
privately chat after they have each other's public keys:

Alice would add bob to her contact list in her own config file:
```toml
[contact.”bob”]
contact_pubkey = “D6UzKA6qCG5Mep16i6pJYkUCQcnp46E1jPBsUhyJiXhb”
```

And Bob would do the same:
```toml
[contact.”alice”]
contact_pubkey = “9sfMEVLphJ4dTX3SEvm6NBhTbWDqfsxu7R2bo88CtV8g”

```


Lets see an Example where 'alice' sends “Hi” message to 'bob' using 
the /msg command  
     
     /msg bob Hi


<u>Note for Weechat Client Users:</u>\
When you private message someone as shown above, the buffer will not 
pop in weechat client until you receive a reply from that person.

For example here 'alice' will not see any new buffer on her irc interface for 
the recent message which she just send to 'bob' until 'bob' replies,
but 'bob' will get a buffer shown on his irc client with the message 'Hi'.      

Reply from 'bob' to 'alice' 

    /msg alice welcome!


Or instead of `/msg` command, you can use:
```
/query bob hello
```
This works exactly the same as `/msg` except it will open a new buffer 
with Bob in your client regardless.

<u><b>Note</b></u>: The contact name is not the irc nickname, it can 
be anything you want, and you should use it when DMing.

<u><b>Note</b></u>: It's always a good idea to save your keys somewhere safe, but in 
case you lost your Public Key and you still have your Private key in 
`ircd_config.toml` file, you recover the Public Key like so:
```shell
% ircd --recover-pubkey {your_private_key}
```

