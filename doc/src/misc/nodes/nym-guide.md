# Run DarkIRC through a Nym SOCKS5 proxy

DarkFi supports outbound clearnet TLS connections through a Nym-provided
SOCKS5 proxy. This is an outbound-only DarkIRC configuration; it does not make
the node a Nym service or advertise an inbound address.

Install and initialize a current Nym SOCKS5 client using Nym's upstream
documentation. Nym's CLI names and provider-selection workflow can change, so
verify those details against the installed version. The selected provider must
permit connections to the DarkIRC P2P port. Start the client and confirm its
local SOCKS5 listener, commonly `127.0.0.1:1080`.

Configure DarkIRC as follows:

```toml
[net]
active_profiles = ["socks5+tls"]
mixed_profiles = ["tcp+tls"]
nym_socks5_proxy = "socks5://127.0.0.1:1080"

[net.profiles."socks5+tls"]
seeds = [
    "socks5+tls://127.0.0.1:1080/lilith0.dark.fi:9600",
    "socks5+tls://127.0.0.1:1080/lilith1.dark.fi:9600",
]
```

The explicit seed URLs bootstrap through the Nym proxy. Declaring `tcp+tls` in
`mixed_profiles` lets learned `tcp+tls` peer addresses be converted to the
active `socks5+tls` transport using `nym_socks5_proxy`.

Do not add `tcp+tls` itself to `active_profiles` unless direct clearnet
connections are also intended. Restart DarkIRC after changing network
settings, and inspect sessions with `dnet` to confirm connections use
`socks5+tls` rather than direct `tcp+tls`.
