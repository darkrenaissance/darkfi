# Sessions

To deploy the p2p network, we need to configure two types of nodes:
inbound and outbound. These nodes perform different roles on the p2p
network. An inbound node receives connections. An outbound node makes
connections.

The behavior of these nodes is defined in what is called a
[Session](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/session/mod.rs#L119).
There are five types of sessions: `Manual`, `Inbound`, `Outbound`, `SeedSync` and `Direct`.

There behavior is as follows: 

**Inbound**: Uses an `Acceptor` to accept connections on the inbound connect
address configured in settings.

**Outbound**: Starts a connect loop for every connect slot configured in
settings. Establishes a connection using `Connector.connect`: a method
that takes an address returns a `Channel`.

**Manual**: Uses a `Connector` to connect to a single address that is passed
to `ManualSession::connect`. Used to create an explicit connection to
a specified address.

**SeedSync**: Creates a connection to the seed nodes specified in settings.
Loops through all the configured seeds and tries to connect to them
using a `Connector`. Either connects successfully, fails with an error or
times out.

**Direct**: Creates a connection to a single address using 
`DirectSession::create_channel`. The address may or may not already be in a
hostlist. Once the channel is stopped this session will not try to reconnect.
Used by protocols to create a temporary connection to a specific address.
