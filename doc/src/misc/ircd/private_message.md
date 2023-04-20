
# Configuring a Private chat between users

Any two users on the ircd server can establish a fully encrypted
 communication medium between each other using a basic keypair setup.

## Configuring ircd_config.toml

`ircd_config.toml` should be created by default in `~/.config/darkfi/`
when you first run `ircd`.

Generate a keypair using the command 

```shell
% ircd --gen-keypair
```
This is the Private key used for encryption of messages across the network.

Save the privateKey safely & add it to the `ircd_config.toml` file as shown below.

```toml
[private_key.”your_private_key_goes_here”]
```

To share your public key with a user over the ircd server you can use the following command. 

     /query User_A  “Hi this is my publickey: XXXXXX"

Note: This message will be publically visible on the IRC chat 
       i.e anyone running the irc demon can view these messages in their logs. 

See the [example ircd_config.toml](https://github.com/darkrenaissance/darkfi/blob/master/bin/ircd/ircd_config.toml) for more details

## Example
Lets start by configuring our contacts list in the generated `ircd_config.toml` file
(you can also refer to the examples written in the comments of the toml file)

```toml
[contact.”User_A”]
contact_pubkey = “XXXXXXX”
[contact.”User_B”]
contact_pubkey = “YYYYYYY”
```

<u>Note</u>: After configuring our `ircd_config.toml` file, you will need to restart your irc demon for the changes to reflect. 


Lets see an Example where 'User_A' sends “Hi” message to 'User_B' using the /msg command  
     
     /msg User_B Hi

IRCD logs of 'User_A'
```
9:36:59 [INFO] [CLIENT 127.0.0.1:xxxx] Msg: PRIVMSG User_B :Hi
09:36:59 [INFO] [CLIENT 127.0.0.1:xxxx] (Plain) PRIVMSG User_B :Hi
09:36:59 [INFO] [CLIENT 127.0.0.1:57964] (Encrypted) PRIVMSG: Privmsg { id: 12345, nickname: “xxxxxxx”, target: “xxxxx”, message: “xxxxxx”, timestamp: 1665481019, term: 0, read_confirms: 0 }
09:36:59 [INFO] [P2P] Broadcast: Privmsg { id: 7563042059426128593, nickname: “xxxx”, target: “xxxxx”, message: “xxxx”, timestamp: 1665481019, term: 0, read_confirms: 0 }
```
IRCD logs of 'User_B'
```
09:36:59 [INFO] [P2P] Received: Privmsg { id: 123457, nickname: “xxxx”, target: “xxxx”, message: “xxxx”, timestamp: 1665481019, term: 0, read_confirms: 0 }
09:36:59 [INFO] [P2P] Decrypted received message: Privmsg { id: 123457, nickname: "User_A", target: "User_B", message: "Hi", timestamp: 1665481019, term: 0, read_confirms: 0 }    
```
<u>Note for Weechat Client Users:</u> 
 When you private message somone as shown above,
the buffer will not pop in weechat client until you receive a
 reply from that person.
For example here 'User_A' will not see any new buffer on his irc interface for
 the recent message which he just send to 'User_B' until 'User_B' replies,
but 'User_B' will get a buffer shown on his irc client with the message 'Hi'.      

Reply from 'User_B' to 'User_A' 

    /msg User_A welcome!

IRCD logs of 'User_B' 
```
10:25:45 [INFO] [CLIENT 127.0.0.1:57396] Msg: PRIVMSG User_A :welcome! 
10:25:45 [INFO] [CLIENT 127.0.0.1:57396] (Plain) PRIVMSG User_A :welcome! 
10:25:45 [INFO] [CLIENT 127.0.0.1:57396] (Encrypted) PRIVMSG: Privmsg { id: 123458, nickname: “xxxx”, target: “xxxx”, message: “yyyyyyy”, timestamp: 1665483945, term: 0, read_confirms: 0 }
10:25:45 [INFO] [P2P] Broadcast: Privmsg { id: 123458, nickname: “xxxxx”, target: “xxxxx”, message: “yyyyyyyy”, timestamp: 1665483945, term: 0, read_confirms: 0 }
```
IRCD logs of 'User_A'
```
10:25:46 [INFO] [P2P] Received: Privmsg { id: 123458, nickname: “xxxxxxx”, target: “xxxxxx”, message: “yyyyyy”, timestamp: 1665483945, term: 0, read_confirms: 0 }
10:25:46 [INFO] [P2P] Decrypted received message: Privmsg { id: 123458, nickname: "User_B”, target: "User_A”, message: "welcome! ", timestamp: 1665483945, term: 0, read_confirms: 0 }
```    