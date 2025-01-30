# DarkIRC: Strong Anonymity P2P Chat

In DarkFi, we organize our communication using resilient and
censorship-resistant infrastructure. For chatting, `darkirc` is a
peer-to-peer implementation of an IRC server in which any user can
participate anonymously using any IRC frontend and by running the
IRC daemon. `darkirc` uses the DarkFi P2P engine to synchronize chats
between hosts.

## Benefits

* Encrypted using same algorithms as Signal.
* There are no identities. You cannot see who is in the chat.
* Completely anonymous. You can rename yourself easily by using the
  command `/nick foo`. This means all messages are unlinkable.
* God-fearing based CLI without soy gui shit.
* p2p decentralized.
* Optionally run it over Tor or Nym (soon) for network level anonymity.

Therefore this is the world's most strongly anonymous chat in existence.
Nothing else exists like it.

<u><b>Note</b></u>: `darkirc` follows IRC's [RFC2812](https://www.rfc-editor.org/rfc/rfc2812)

## Building

Follow the instructions in the [README](../../index.html#build) to ensure
you have all the necessary dependencies. After that, in repo root folder:

```shell
% make darkirc
```

## Installation (Optional)

It is adviced to use `darkirc` directly from the repo root folder.
Install system wide only if you can make sure there would be no
multiple darkirc versions installed:

```shell
% sudo make install darkirc
```

You have to reinstall `darkirc` on new versions manually.

## Building for Android

This is for Android 64 bit (which is most phones).
You will compile darkirc on your computer then copy it to your phone
and run it in Termux (a command-line terminal for Android).

We will use podman which is a secure replacement for docker. However if you
prefer to use docker just be aware of
[the security risks](https://docs.docker.com/engine/security/#docker-daemon-attack-surface).
Podman is a drop in replacement.

1. Setup podman on your computer which may look like:
    1. Install podman package
    2. Run the podman daemon service under your local user
        1. Use the command `podman system service`.
        2. For Docker it's more complicated, see [rootless mode](https://docs.docker.com/engine/security/rootless/).
2. Run `cd bin/darkirc/ && make podman-android`. The resulting file 
    will be called `darkirc.aarch64-android` (it might be needed to 
    make the file executable `chmod +x darkirc.aarch64-android`). 
    Copy this to your phone.
3. Install Termux and RevolutionIRC on F-Droid.
4. Run `termux-setup-storage` and allow access to external storage.
   Now you can access the phone storage from `/sdcard/` and copy the file
   into the Termux home.
5. Run `termux-wake-lock`. This stops Android suspending the daemon.
6. Run the daemon. You can open new Termux sessions by swiping from
   the left to bring up the sidebar.
7. Connect the RevolutionIRC frontend by adding a new server:
    1. Write a name for the server (i.g `darkirc`).
    2. Set the server address and port (if using default config these 
        should be 127.0.0.1:6667).
    3. Untick `Use SSL/TLS` option.
    4. Save and connect.

## Logs

The public channels have [logs available](https://agorism.dev/log/), and
additionally there is a mirror on telegram @darkfi_darkirc channel.
You can also message @darkirc_bot with "sub" to avoid doxxing your username.
Use "unsub" to unsubscribe.

## Network-level privacy

Nodes have knowledge of their peers, including the IP addresses of
connected hosts. We suggest configuring your instance to use a different
transport so it is not connected via clearnet.

DarkFi supports the use of pluggable transports, including [Tor](../nodes/tor-guide.md#configure-network-settings)
and Nym, to provide network-level privacy. As long as there are live seed
nodes configured to support a Tor or Nym connection, users can connect to
`darkirc` and benefit from the protections offered by these protocols.

Other approaches include connecting via a cloud server or VPN. Research
the risks involved in these methods before connecting.

## Usage (DarkFi Network)

Upon compiling `darkirc` as described above, the preconfigured defaults
will allow you to connect to the network and start chatting with the
rest of the DarkFi community.

First, try to start `darkirc` from your command-line so it can spawn its
configuration file in place. The preconfigured defaults will autojoin
you to several default channels one of which is `#dev` where we have 
weekly meetings, and where the community is most active and talks 
about DarkFi development.

```shell
% ./darkirc
```

`darkirc` will create a configuration file `darkirc_config.toml` by 
default in `~/.config/darkfi/` you can review and potentially edit. It 
might be useful if you want to add other channels you want to autojoin 
(like `#philosophy` and `#memes`), or if you want to set a shared 
secret for some channel in order for it to be encrypted between its 
participants. We strongly suggest to make sure you are using the
desired network transport before proceeding.

When done, you can run `darkirc` for the second time in order for it to
connect to the network and start participating in the P2P protocol:

```shell
% ./darkirc
```

The daemon will start conncting to peers and sync its database, you'll 
know it's finished syncing when you see this log message:
```shell
% [EVENTGRAPH] DAG synced successfully!
```

Now connect your favorite IRC client and it should replay missed 
messages that have been sent by people.


## Clients

### Weechat

In this section, we'll briefly cover how to use the [Weechat IRC
client](https://github.com/weechat/weechat) to connect and chat with
`darkirc`.

Normally, you should be able to install weechat using your
distribution's package manager. If not, have a look at the weechat
[git repository](https://github.com/weechat/weechat) for instructions
on how to install it on your computer.

Once installed, we can configure a new server which will represent our
`darkirc` instance. First, start weechat, and in its window - run the
following commands (there is an assumption that `irc_listen` in the
`darkirc` config file is set to `127.0.0.1:6667`):

```
/server add darkfi localhost/6667 -notls -autoconnect
/save
/quit
```

This will set up the server, save the settings, and exit weechat.
You are now ready to begin using the chat. Simply start weechat
and everything should work.

When you join, you should see users nicknames on the right panel.
those nicknames are users who previously sent messages and you got 
those messages as history when you synced.
Normally nicks would not be shown since there is no concept of 
nicknames or registration on this p2p anonymous chat.

You can change your nickname using `/nick foo`, and navigate channels
using F5/F6 or ALT+X where X is the channel number displayed.
You can also use ALT+up/down.

Whenever you edit `darkirc_config.toml` file and if you have your 
`darkirc` daemon running you don't need to restart it to reload the 
config, you just need to send a `rehash` command from IRC client for 
the changes to reflect, like so:

```
/rehash
```

## Usage (Local Deployment)

These steps below are only for developers who wish to make a testing
deployment. The previous sections are sufficient to join the chat.

### Seed Node

First you must run a seed node. The seed node is a static host which
nodes can connect to when they first connect to the network. The
`seed_session` simply connects to a seed node and runs `protocol_seed`,
which requests a list of addresses from the seed node and disconnects
straight after receiving them.

The first time you run the program, a config file will be created in
`~/.config/darkfi` if you are using Linux or in 
`~/Library/Application Support/darkfi/` on MacOS. 
You must specify an inbound accept address in your config file to configure a seed node:

```toml
## P2P accept addresses
inbound=["127.0.0.1:11001"]
```

Note that the above config doesn't specify an external address since
the seed node shouldn't be advertised in the list of connectable
nodes. The seed node does not participate as a normal node in the
p2p network. It simply allows new nodes to discover other nodes in
the network during the bootstrapping phase.

### Inbound Node

This is a node accepting inbound connections on the network but which
is not making any outbound connections.

The external addresses are important and must be correct.

To run an inbound node, your config file must contain the following
info:
		
```toml
## P2P accept addresses
inbound=["127.0.0.1:11002"]

## P2P external addresses
external_addr=["127.0.0.1:11002"]

## Seed nodes to connect to 
seeds=["127.0.0.1:11001"]
```
### Outbound Node

This is a node which has 8 outbound connection slots and no inbound
connections.  This means the node has 8 slots which will actively
search for unique nodes to connect to in the p2p network.

In your config file:

```toml
## Connection slots
outbound_connections=8

## Seed nodes to connect to 
seeds=["127.0.0.1:11001"]
```

### Attaching the IRC Frontend

Assuming you have run the above 3 commands to create a small model
testnet, and both inbound and outbound nodes above are connected,
you can test them out using weechat.

To create separate weechat instances, use the `--dir` command:

    weechat --dir /tmp/a/
    weechat --dir /tmp/b/

Then in both clients, you must set the option to connect to temporary
servers:

    /set irc.look.temporary_servers on

Finally you can attach to the local darkirc instances:

    /connect localhost/6667
    /connect localhost/6668

And send messages to yourself.

### Running a Fullnode

See the script `script/run_node.sh` for an example of how to deploy
a full node which does seed session synchronization, and accepts both
inbound and outbound connections.

## Global Buffer

Copy [this script](https://github.com/narodnik/weechat-global-buffer/blob/main/buffclone.py) 
to `~/.local/share/weechat/python/autoload/`, and you will create a single buffer 
which aggregates messages from all channels. It's useful to monitor 
activity from all channels without needing to flick through them.

You may need to install `weechat-python` to enable Python scripting support
in your weechat.

## Emojis

Install the `noto` fonts to have the full unicode set. Popular Linux distros
should have packages for them.

Once installed you can view all the emojis in your terminal. Note, you may need
to regenerate your font cache (or just restart) after installing them.

## Further Customization

Group channels under respective networks:

```
/set irc.look.server_buffer independent
/set irc.look.new_channel_position near_server
```

Filter all join-part-quit messages (only relevant for other networks):

```
/set irc.look.smart_filter on
/filter add joinquit * irc_join,irc_part,irc_quit,irc_nick,irc_account,irc_chghost *
```

For customizing the colors, see
[this article](https://blog.swwomm.com/2020/07/weechat-light-theme.html).

### Settings Editor

Make sure you run `/save`, `/quit` to reload your config after these changes.

To see the Weechat settings editor, simply type `/set` in the main buffer.
You can then type prefixes like "autojoin" and press enter to find all settings
related to that. To change it type ALT+enter. Everything in Weechat is
customizable!

The help is your friend. Every command has help.

```
/help key
/help server
```

For example to set the shortcut ALT-w to close a buffer,
use `/key bind meta-w /close`.

### Other IRC Networks

For more fun, you can join Libera IRC. Note this may potentially dox your node,
especially if you have autoconnect enabled since Libera is not anon.

```
/server add libera irc.libera.chat/6697 -ssl -autoconnect
/save
/connect libera
/join #rust
/join #linux
/join #math
```

You can find more channels with `/list`. Then add your favorite channels to the
libera autojoin list.

Note that your nick is temporary. If you want to claim a nick, you will need to
[register with the NickServer](https://libera.chat/guides/registration).

## Troubleshooting

If you encounter connectivity issues refer to 
[Network troubleshooting](../network-troubleshooting.md)
for further troubleshooting resources.
