# Node configurations

This section provides configuration examples for hosting DarkFi P2P nodes.

## Default Ports

### `darkfid` Mainnet
- `8340`: Inbound P2P
- `_ +1`: Inbound Tor/I2p/...
- `8345`: Public RPC server
- `8346`: Restricted RPC server
- `8347`: Stratum RPC server
- `8348`: P2pool Merge Mining RPC server


###  `darkfid` Testnet
- `18340`: Inbound clear P2P
- `_ + 1`: Inbound Tor/I2p/...
- `18345`: General query RPC server
- `18346`: Restricted RPC server
- `18347`: Stratum RPC server
- `18348`: P2pool Merge Mining RPC server


###  `darkirc`
- `6667`: IRC server (plaintext by default)
- `6697`: Conventional IRC TLS port (only when configured)
- `9600`: Inbound P2P
- `_ +1`: Inbound Tor/I2p/...
- `9605`: RPC server

DarkIRC does not enable an inbound P2P listener by default. It currently has
one RPC listener rather than separate public and restricted RPC servers.


### `fud`
- `9700`: Inbound P2P
- `_ +1`: Inbound Tor/I2p/...
- `9705`: Public RPC server
- `9706`: Restricted RPC server


### `taud`
- `9800`: Inbound P2P
- `_ +1`: Inbound Tor/I2p/...
- `9805`: Public RPC server
- `9806`: Restricted RPC server
