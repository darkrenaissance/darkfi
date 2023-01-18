# Sessions

To deploy the p2p network, we need to configure two types of nodes:
inbound and outbound. These nodes perform different roles on the p2p
network. An inbound node receives connections. An outbound node makes
connections.

The behavior of these nodes is defined in what is called a
[Session](https://github.com/darkrenaissance/darkfi/blob/master/src/net/session/mod.rs#L111).
There are four types of sessions: `Manual`, `Inbound`, `Outbound` and `SeedSync`.

There behavior is as follows: 

**Inbound**: Uses an `Acceptor` to accept connections on the inbound connect
address configured in settings.

**Outbound**: Starts a connect loop for every connect slot configured in
settings. Establishes a connection using `Connector.connect()`: a method
that takes an address returns a `Channel`.

**Manual**: Uses a `Connector` to connect to a single address that is passed
to `ManualSession::connect()`. Used to create an explicit connection to
a specified address.

**SeedSync**: Creates a connection to the seed nodes specified in settings.
Loops through all the configured seeds and tries to connect to them
using a `Connector`. Either connects successfully, fails with an error or
times out.
