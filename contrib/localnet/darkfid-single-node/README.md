darkfid localnet
================

This will start one `darkfid` node in localnet mode,
along with a `minerd` daemon to mine blocks.

If we want to test wallet stuff, we must generate
a testing wallet and pass its address to the `darkfid`
config, so the wallet gets the block rewards the node
produces. First we start `darkfid` and wait until its
initialized:
```
% ./tmux_sessions.sh
```

In another terminal, we generate a wallet, set it as
the default and grab its address:
```
% ../../../drk -c drk.toml wallet --initialize
% ../../../drk -c drk.toml wallet --keygen
% ../../../drk -c drk.toml wallet --default-address 1
% ../../../drk -c drk.toml wallet --address
```
Then we replace the `recipient` field in `darkfid.toml`
config with the output of the last command, and restart
the daemon. After some blocks have been generated we
will see some `DRK` in our test wallet.

```
% ../../../drk -c drk.toml scan
% ../../../drk -c drk.toml wallet --balance
```

See the user guide in the book for more info.

