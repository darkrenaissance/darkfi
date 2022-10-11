
# Configuring a Private chat between users

Ircd provides a fully decentralized P2P encrypted chat between users.


## Configuring ircd_config.toml

Generate a keypair using the command 

```shell
% ircd --gen-kepair
```
This is the Private key used to decrypt direct messages to you.

save this privateKey safely and add this to the ircd_config.toml file as shown below

```toml
[private_key.”your_private_key_goes_here”]
```

to share public key with a user you can use the command 

     /query User_A  “Hi this is my  publickey: XXXXXX"

Note : this messgae will be publically visible on the IRC chat anyone running the irc demon can view this in logs 

See the [example ircd_config.toml](https://github.com/darkrenaissance/darkfi/blob/master/bin/ircd/ircd_config.toml) for more details

## Example
 Lets First configure the contacts in the ircd_config.toml (refer to the examples files already given in the comments of the file)
    User_A and User_B 

```toml
[contact.”User_A”]
contact_pubkey = “XXXXXXX”
[contact.”User_B”]
contact_pubkey = “YYYYYYY”
```

Note : after configuring the ircd_config.toml , you will need to restart your irc demon for changes to reflect 


Lets see an Example where User_A sends “Hi” to User_B using the /msg command  
     
     /msg User_B Hi

IRCD logs of User_A

    9:36:59 [INFO] [CLIENT 127.0.0.1:xxxx] Msg: PRIVMSG User_B :Hi
    09:36:59 [INFO] [CLIENT 127.0.0.1:xxxx] (Plain) PRIVMSG User_B :Hi
    09:36:59 [INFO] [CLIENT 127.0.0.1:57964] (Encrypted) PRIVMSG: Privmsg { id: 12345, nickname: “xxxxxxx”, target: “xxxxx”, message: “xxxxxx”, timestamp: 1665481019, term: 0, read_confirms: 0 }
    09:36:59 [INFO] [P2P] Broadcast: Privmsg { id: 7563042059426128593, nickname: “xxxx”, target: “xxxxx”, message: “xxxx”, timestamp: 1665481019, term: 0, read_confirms: 0 }

IRCD logs of User_B

    09:36:59 [INFO] [P2P] Received: Privmsg { id: 123457, nickname: “xxxx”, target: “xxxx”, message: “xxxx”, timestamp: 1665481019, term: 0, read_confirms: 0 }
    09:36:59 [INFO] [P2P] Decrypted received message: Privmsg { id: 123457, nickname: "User_A", target: "User_B", message: "Hi", timestamp: 1665481019, term: 0, read_confirms: 0 }    

Note for Weechat Client Users: once you messgae the buffer will not pop in weechat client unless you recieve a reply from that person. Here User_A will not see any new buffer opened for the recent message until User_B replies, but the User_B will have a buffer shown on the client      

Reply from User_B to User_A 

    /msg User_A welcome!

IRCD logs of User_B 

    10:25:45 [INFO] [CLIENT 127.0.0.1:57396] Msg: PRIVMSG User_A :welcome! 
    10:25:45 [INFO] [CLIENT 127.0.0.1:57396] (Plain) PRIVMSG User_A :welcome! 
    10:25:45 [INFO] [CLIENT 127.0.0.1:57396] (Encrypted) PRIVMSG: Privmsg { id: 123458, nickname: “xxxx”, target: “xxxx”, message: “yyyyyyy”, timestamp: 1665483945, term: 0, read_confirms: 0 }
    10:25:45 [INFO] [P2P] Broadcast: Privmsg { id: 123458, nickname: “xxxxx”, target: “xxxxx”, message: “yyyyyyyy”, timestamp: 1665483945, term: 0, read_confirms: 0 }

IRCD logs of User_A

    10:25:46 [INFO] [P2P] Received: Privmsg { id: 123458, nickname: “xxxxxxx”, target: “xxxxxx”, message: “yyyyyy”, timestamp: 1665483945, term: 0, read_confirms: 0 }
    10:25:46 [INFO] [P2P] Decrypted received message: Privmsg { id: 123458, nickname: "User_B”, target: "User_A”, message: "welcome! ", timestamp: 1665483945, term: 0, read_confirms: 0 }