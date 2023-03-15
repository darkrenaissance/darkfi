
## Prerequisites
Download `nym-network-requester` and `nym-socks5-client` binaries 
from: https://github.com/nymtech/nym/releases
(version 1.1.10 was tested)

## initializing and run nym bins:
- ### network requester:
```
# --gateway is optional
# gateway address is an already existing one
# if omitted, gateway will be randomly chosen based on some factors
% ./nym-network-requester init --id nettestnode --gateway 2BuMSfMW3zpeAjKXyKLhmY4QW1DXurrtSPEJ6CjX3SEh
% ./nym-network-requester run --id nettestnode
```
take note of the address in the log, will be something like this:

> \> The address of this client is: 4owvZtatuzYJkJFmG6xyAxCg7ic7aZiJfBnReb6SbEtf.3F5DS1p1UP9Y6H7AGNnRAeTqXU81dQVj8BTDeVNLZWsp@2BuMSfMW3zpeAjKXyKLhmY4QW1DXurrtSPEJ6CjX3SEh

alternatively you can check the config file:
`~/.nym/service-providers/network-requester/nettestnode/config/config.toml`

- ### client:
```
# --provider is required
% ./nym-socks5-client init --id sockstest --provider 4owvZtatuzYJkJFmG6xyAxCg7ic7aZiJfBnReb6SbEtf.3F5DS1p1UP9Y6H7AGNnRAeTqXU81dQVj8BTDeVNLZWsp@2BuMSfMW3zpeAjKXyKLhmY4QW1DXurrtSPEJ6CjX3SEh
% ./nym-socks5-client run --id sockstest
```

after `nym-network-requester` initialization you can add '`127.0.0.1`' or '`localhost`'
to `~/.nym/service-providers/network-requester/allowed.list`

adding a new domain/address to `allowed.list` while `nym-network-requester` is running
will require you to restart it.


now you can run tmux_session.sh

