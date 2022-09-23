# get_info

If you run Alice now, you'll see the following output:

```
[DEBUG] jsonrpc-server: Trying to bind listener on tcp://127.0.0.1:55054
```

That indicates that our JSON-RPC server is up and running. However,
there's currently no client for us to connect to. That's where `dnetview`
comes in. `dnetview` implements a JSON-RPC client that calls a single
method: `get_info()`.

To use it, let's return to our `JsonRpcInterface` and add the following
method:

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:45:52}}
```

And add it to `handle_request()`:

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:21}}
        //...
{{#include ../../../../../example/dchat/src/rpc.rs:28:34}}
```

This calls the p2p function `get_info()` and passes the returned data into a
`JsonResponse`.

Under the hood, this function triggers a hierarchy of `get_info()`
calls which deliver info specific to a node, its inbound or outbound
`Session`'s, and the `Channel`'s those `Session`'s run.

Here's what happens:

```rust
{{#include ../../../../../src/net/p2p.rs:111:126}}
```

Here we return two pieces of info that are unique to a node:
`external_addr` and `state`. We couple that data with `SessionInfo`
by calling `get_info()` on each `Session`.

`Session::get_info()` returns data related to a `Session`
(for example, an Inbound `accept_addr` in the case of an
inbound `Session`). `Session::get_info()` then calls the function
`Channel::get_info()` which returns data specific to a `Channel`. This
happens via a child struct called `ChannelInfo`.

This is `ChannelInfo::get_info()`.

```rust
{{#include ../../../../../src/net/channel.rs:48:58}}
```

`dnetview` uses the info returned from `Channel` and `Session` and
node-specific info like `external_addr` to display an overview of the
p2p network.
