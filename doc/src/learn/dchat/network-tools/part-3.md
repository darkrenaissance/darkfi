# Network tools

In its current state, dchat is ready to use. But there's steps we can
take to improve it. If we connect dchat to JSON-RPC, we gain access to
a tool called `dnetview` that allows us to visually explore connections
and messages on the p2p network.

As well as facilitating debugging, connecting
`dnetview` is a good excuse to dive into DarkFi's [rpc
module](https://github.com/darkrenaissance/darkfi/tree/master/src/rpc)
which is essential to the DarkFi code base.

This section will cover:

* DarkFi's JSON-RPC interface
* Exploring the p2p network topology using `dnetview`

