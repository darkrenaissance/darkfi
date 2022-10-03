
# Ircd Specification

Ircd use [Hashchain](https://darkrenaissance.github.io/darkfi/misc/hashchain/hashchain.html)
to maintain the synchronization between nodes. The messages are handled as
events in Ircd network.


## PrivMsgEvent

This is the main message type inside Ircd. The `PrivMsgEvent` is an
[event action](https://darkrenaissance.github.io/darkfi/misc/hashchain/hashchain.html#eventaction).


| Description	| Data Type		| Comments																	| 
|-------------- |-------------- | ------------------------------------------------------------------------- |
| nickname		| String		| The nickname for the sender (must be less than 32 chars)					|
| target		| String		| The target for the message (recipient)									|
| message		| String		| The actual content of the message											|

## ChannelInfo

Preconfigured channel in the configuration file.

In the TOML configuration file, the channel is set as such:

```toml
[channel."#dev"]
secret = "GvH4kno3kUu6dqPrZ8zjMhqxTUDZ2ev16EdprZiZJgj1"
topic = "DarkFi Development Channel"
```

| Description	| Data Type		| Comments																	|
|-------------- |-------------- | ------------------------------------------------------------------------- |
| topic			| String		| Optional topic for the channel											|
| secret		| String		| Optional NaCl box for the channel, used for {en,de}cryption.				|
| joined		| bool			| Indicate whether the user has joined the channel							|
| names			| Vec<String>	| All nicknames which are visible on the channel							|


## ContactInfo

Preconfigured contact in the configuration file.

In the TOML configuration file, the contact is set as such:

```toml
[contact."nick"]
pubkey = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
```

| Description	| Data Type		| Comments												|
|-------------- |-------------- | ----------------------------------------------------- |
| pubkey		| String		| A Public key for the contact to encrypt the message	|

## IrcConfig

The base Irc configuration for each new `IrcClient`.

| Description	| Data Type						  | Comments																		|
|-------------- |-------------------------------- | ------------------------------------------------------------------------------- |
| is_nick_init	| bool							  | Confirmation of receiving /nick command											|
| is_user_init	| bool							  | Confirmation of receiving /user command											|
| is_cap_end	| bool							  | Indicate whether the irc client finished the Client Capability Negotiation		|
| is_pass_init	| bool							  | Confirmation of checking the password in the configuration file					|
| is_registered | bool							  | Indicate the `IrcClient` is initialized and ready to sending/receiving messages	|
| nickname		| String						  | The irc client nickname															|
| password		| String						  | The password for the irc client. (it could be empty)							|
| private_key	| Option<String>				  | A private key to decrypt direct messages from contacts							|
| capabilities	| HashMap<String, bool>			  | A list of capabilities for the irc clients and the server to negotiate			|
| auto_channels	| Vec<String>					  | Auto join channels for the irc clients											|
| channels		| HashMap<String, `ChannelInfo`>  | A list of preconfigured channels in the configuration file						|
| contacts		| HashMap<String, `ContactInfo`>  | A list of preconfigured contacts in the configuration file for direct message	|

## IrcServer

The server start listening to an address specifed in the configuration file. 

For each irc client get connected, an `IrcClient` instance created.

A Communication channel get initialized by the server for the new `IrcClient` 
and add to the subscriptions list for handling data between them. 
in this way the server handle and maintain each irc client connection separately.  

| Description				| Data Type						| Comments												|
|-------------------------- |------------------------------ | ----------------------------------------------------- |
| settings					| `Settings`					| The base settings parsed from the configuration file  |
| clients_subscriptions		| SubscriberPtr<`PrivMsgEvent`> | Channels to notify the `IrcClient`s about new messages|

##  IrcClient

The `IrcClient` handle all irc opeartions and commands from the irc client.

The subscription channel listen to the server for new messages from the ircd 
network and pass them to the irc client. While notify the server to broadcast 
new messages from the irc client.


| Description		| Data Type							| Comments																	|
|-------------------------- |------------------------------ | ----------------------------------------------------- |
| write_stream		| WriteHalf<Stream>					| A writer for sending data to the connection stream						|
| read_stream		| ReadHalf<Stream>					| Read data from the connection stream										|
| address			| SocketAddr						| The actual address for the irc client connection							|
| irc_config		| `IrcConfig`						| Base configuration for irc												|
| server_notifier 	| Channel<(`PrivMsgEvent`, u64)> 	| A Channel to notify the server about a new message from the irc client	|
| subscription 		| Subscription<`PrivMsgEvent`> 		| A channel to receive messages from the server 							|




