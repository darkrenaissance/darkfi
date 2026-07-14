# Run an I2P-enabled DarkIRC node

DarkFi reaches I2P services through an I2P SOCKS5 proxy. Install and configure
an I2P router such as i2pd using its current upstream documentation, and verify
that its SOCKS5 proxy is listening (commonly on `127.0.0.1:4447`).

DarkIRC's I2P profile is not enabled by default and the template does not
provide a guaranteed official I2P seed. Obtain a current DarkIRC I2P seed or
manual peer from a trusted operator before relying on this profile.

## Outbound I2P node

```toml
[net]
active_profiles = ["i2p"]
i2p_socks5_proxy = "socks5://127.0.0.1:4447"

[net.profiles."i2p"]
seeds = ["i2p://CURRENT_DARKIRC_SEED.b32.i2p:9600"]
```

The `i2p_socks5_proxy` belongs in `[net]`; seed and peer URLs belong in the
`[net.profiles."i2p"]` table.

## Inbound I2P node

Create an I2P server tunnel that forwards an externally reachable I2P port to
a loopback TCP listener. The exact router configuration and file locations are
router- and distribution-specific. If the tunnel maps public port 9600 to
`127.0.0.1:9601`, configure:

```toml
[net]
active_profiles = ["i2p"]
inbound_connections = 64
i2p_socks5_proxy = "socks5://127.0.0.1:4447"

[net.profiles."i2p"]
inbound = ["tcp://127.0.0.1:9601"]
external_addrs = ["i2p://YOUR_ADDRESS.b32.i2p:9600"]
```

Back up the I2P destination keys if the address must remain stable. Restart
DarkIRC after changing the profile, then test the advertised I2P endpoint from
another I2P-connected host.
