# RPC server

To deploy the `JsonRpcInterface`, we'll need to
create an RPC server using `listen_and_serve()`.
`listen_and_serve()` is a method defined in DarkFi's [rpc
module](https://github.com/darkrenaissance/darkfi/tree/master/src/rpc/server.rs).
It starts a JSON-RPC server that is bound to the provided accept URL
and uses our previously implemented `RequestHandler` to handle incoming
requests.

Add the following lines to `main()`:

```rust
{{#include ../../../../../example/dchat/src/main.rs:json_init}}
```

We create a new `JsonRpcInterface` inside an `Arc` pointer and pass in our
`accept_addr` and `p2p` object.

Next, we create an async block that calls `listen_and_serve()`. The async
block uses the `move` keyword to takes ownership of the `accept_addr`
and `JsonRpcInterface` values and pass them into `listen_and_serve()`.
We use an `executor` to spawn `listen_and_serve()` as a new thread and
detach it in the background.

We have enabled JSON-RPC.

Here's what our complete `main()` function looks like:

```rust
{{#include ../../../../../example/dchat/src/main.rs:main}}
```
