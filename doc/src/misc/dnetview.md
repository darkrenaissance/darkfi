# dnet

`dnet` is the Python TUI for inspecting DarkFi P2P sessions and messages. The
old `dnetview` binary and `make BINS=dnetview` instructions no longer apply.

See the maintained [dnet guide](../learn/dchat/network-tools/using-dnet.md).
For DarkIRC, configure dnet to connect to RPC port 9605 and remove
`"p2p.get_info"` from `rpc_disabled_methods` before restarting DarkIRC.
