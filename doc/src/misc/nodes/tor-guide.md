# Run a Tor-enabled DarkIRC node

DarkFi includes an Arti-based `tor` transport. It can make outbound onion
connections without a separately running Tor daemon. Tor hides the clearnet
address used to reach DarkIRC peers, subject to Tor's own threat model; it does
not encrypt public channel contents or make chat metadata unlinkable.

## Outbound-only Tor node

The generated DarkIRC configuration uses this mode by default:

```toml
[net]
active_profiles = ["tor"]

[net.profiles."tor"]
seeds = [
    "tor://g7fxelebievvpr27w7gt24lflptpw3jeeuvafovgliq5utdst6xyruyd.onion:9600",
    "tor://yvklzjnfmwxhyodhrkpomawjcdvcaushsj6torjz2gyd7e25f3gfunyd.onion:9600",
]
```

No `inbound` or `external_addrs` setting is needed for an outbound-only node.

## Inbound Tor node

Inbound service improves network capacity. Choose an ephemeral Arti service or
a static onion service.

### Ephemeral Arti service

Add a `tor://` listener to the Tor profile:

```toml
[net.profiles."tor"]
inbound = ["tor://127.0.0.1:9601"]
```

Arti creates and advertises an ephemeral onion service. Do not add a guessed
`external_addrs` entry. The onion identity is not intended to survive service
recreation, so this mode is unsuitable for a stable seed address.

### Static onion service

For a persistent onion identity, install and configure a Tor daemon according
to the current Tor documentation. Map public onion port 9600 to a loopback
listener, for example in `torrc`:

```text
HiddenServiceDir /var/lib/tor/darkfi_darkirc
HiddenServicePort 9600 127.0.0.1:9601
```

After Tor creates the service, read its hostname using the permissions of the
Tor service account. Configure DarkIRC to accept the forwarded TCP connection
and advertise the onion address:

```toml
[net]
active_profiles = ["tor"]
inbound_connections = 64

[net.profiles."tor"]
inbound = ["tcp://127.0.0.1:9601"]
external_addrs = ["tor://YOUR_ONION_ADDRESS.onion:9600"]
```

Back up the Tor hidden-service keys securely if the stable onion identity must
survive migration.

## Use an external Tor SOCKS5 proxy

This is an alternative to direct Arti onion dialing. Put the proxy address in
each explicit SOCKS endpoint:

```toml
[net]
active_profiles = ["socks5"]

[net.profiles."socks5"]
seeds = [
    "socks5://127.0.0.1:9050/g7fxelebievvpr27w7gt24lflptpw3jeeuvafovgliq5utdst6xyruyd.onion:9600",
    "socks5://127.0.0.1:9050/yvklzjnfmwxhyodhrkpomawjcdvcaushsj6torjz2gyd7e25f3gfunyd.onion:9600",
]
```

Confirm the proxy is listening before starting DarkIRC. Avoid accidentally
wrapping a Tor connection in another Tor connection when the host environment
already transparently routes traffic through Tor.

Restart DarkIRC after profile changes, wait for DAG sync, and use the
[troubleshooting guide](../network-troubleshooting.md#tor-connections) if onion
connections fail.
