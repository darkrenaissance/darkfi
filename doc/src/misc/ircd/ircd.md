# ircd: Strong Anonymity P2P Chat

In DarkFi, we organize our communication using resilient and
censorship-resistant infrastructure. For chatting, `ircd` is a
peer-to-peer implementation of an IRC server in which any user can
participate anonymously using any IRC frontend and by running the
IRC daemon. `ircd` uses the DarkFi P2P engine to synchronize chats
between hosts.

## Benefits

* Encrypted using same algorithms as Signal.
* There are no identities. You cannot see who is in the chat.
* Completely anonymous. You can rename yourself easily by using the
  command `/nick foo`. This means all messages are unlinkable.
* God-fearing based CLI without soy gui shit.
* p2p decentralized.
* Optionally run it over Tor or Nym for network level anonymity.

Therefore this is the world's most strongly anonymous chat in existence.
Nothing else exists like it.

## Installation

Follow the instructions in the
[README](https://darkrenaissance.github.io/darkfi/index.html#build) to ensure
you have all the necessary dependencies.

```shell
% git clone https://github.com/darkrenaissance/darkfi.git
% cd darkfi && git checkout v0.4.1
% make ircd
```

## Installation (Android)

This is for Android 64 bit (which is most phones).

1. Install Docker
2. Run `cd bin/ircd/ && make android`. The resulting file will be called
   `ircd.aarch64-android`. Copy this to your phone.
3. Install Termux and RevolutionIRC on F-Droid.
4. You can access the phone storage from `/sdcard/` and copy the file
   into the Termux home.
5. Run `termux-wake-lock`. This stops Android suspending the daemon.
6. Run the daemon. You can open new Termux sessions by swiping from
   the left to bring up the sidebar.
7. Connect the RevolutionIRC frontend.

## Logs

The public channels have [logs available](https://agorism.dev/log/), and
additionally there is a mirror on telegram @darkfi_darkirc channel.
You can also message @darkirc_bot with "sub" to avoid doxxing your username.
Use "unsub" to unsubscribe.

## Usage (DarkFi Network)

Upon compiling `ircd` as described above, the preconfigured defaults
will allow you to connect to the network and start chatting with the
rest of the DarkFi community.

First, try to start `ircd` from your command-line so it can spawn its
configuration file in place. The preconfigured defaults will autojoin
you to several default channels one of which is `#dev` where we have 
weekly meetings, and where the community is most active and talks 
about DarkFi development.

```shell
% ./ircd
```

`ircd` will create a configuration file `ircd_config.toml` by 
default in `~/.config/darkfi/` you can review and potentially edit. It 
might be useful if you want to add other channels you want to autojoin 
(like `#philosophy` and `#memes`), or if you want to set a shared 
secret for some channel in order for it to be encrypted between its 
participants.

When done, you can run `ircd` for the second time in order for it to
connect to the network and start participating in the P2P protocol:

```shell
% ./ircd
```

## Clients

### Weechat

In this section, we'll briefly cover how to use the [Weechat IRC
client](https://github.com/weechat/weechat) to connect and chat with
`ircd`.

Normally, you should be able to install weechat using your
distribution's package manager. If not, have a look at the weechat
[git repository](https://github.com/weechat/weechat) for instructions
on how to install it on your computer.

Once installed, we can configure a new server which will represent our
`ircd` instance. First, start weechat, and in its window - run the
following commands (there is an assumption that `irc_listen` in the
`ircd` config file is set to `127.0.0.1:6667`):

```
/server add darkfi localhost/6667 -notls -autoconnect
/save
/quit
```

This will set up the server, save the settings, and exit weechat.
You are now ready to begin using the chat. Simply start weechat
and everything should work.

When you join, you will not see any users displayed. This is normal
since there is no concept of nicknames or registration on this p2p 
anonymous chat.

You can change your nickname using `/nick foo`, and navigate channels
using F5/F6 or ALT+X where X is the channel number displayed.

Channels can be moved around using `/buffer move N` where N is the new
position of the buffer in the list. Use `/layout store` to save the
current layout of the buffers.

## Network-level privacy

Nodes have knowledge of their peers, including the IP addresses of 
connected hosts.

DarkFi supports the use of pluggable transports, including Tor and Nym, 
to provide network-level privacy. As long as there are live seed nodes
configured to support a Tor or Nym connection, users can connect to 
`ircd` and benefit from the protections offered by these protocols.

Other approaches include connecting via a cloud server or VPN. Research 
the risks involved in these methods before connecting.

## Running a Fullnode

See the script `script/run_node.sh` for an example of how to deploy
a full node which does seed session synchronization, and accepts both
inbound and outbound connections.

## Global Buffer

Copy [this script](https://github.com/narodnik/weechat-global-buffer/blob/main/buffclone.py) 
to `~/.weechat/python/autoload/`, and you will create a single buffer 
which aggregates messages from all channels. It's useful to monitor 
activity from all channels without needing to look at each one individually.

## Emojis

Install the `noto` fonts to have the full unicode set. Popular Linux distros
should have packages for them.

Once installed you can view all the emojis in your terminal. Note, you may need
to regenerate your font cache (or just restart) after installing them.

