# RPC interface

Let's begin connecting dchat up to JSON-RPC using DarkFi's [rpc
module](https://github.com/darkrenaissance/darkfi/tree/master/src/rpc).

We'll start by defining a new struct called `JsonRpcInterface` that
takes two values, an accept `Url` that will receive JSON-RPC requests,
and a pointer to the p2p network.

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:1:17}}
```

We'll need to implement a trait called `RequestHandler` for
the `JsonRpcInterface`. `RequestHandler` exposes a method called
`handle_request()` which is a handle for processing incoming
JSON-RPC requests. `handle_request()` takes a `JsonRequest`
and returns a `JsonResult`. These types are defined inside
[jsonrpc.rs](https://github.com/darkrenaissance/darkfi/blob/master/src/rpc/jsonrpc.rs)

This is `JsonResult`:
```rust
{{#include ../../../../../src/rpc/jsonrpc.rs:49:55}}
```

This is `JsonRequest`:

```rust
{{#include ../../../../../src/rpc/jsonrpc.rs:75:86}}
```

We'll use `handle_request()` to run a match statement on
`JsonRequest.method`.

Running a match on `method` will allow us to branch out to functions
that respond to methods received over JSON-RPC.  We haven't implemented
any methods yet, so for now let's just return a `JsonError`.

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:19:28}}
{{#include ../../../../../example/dchat/src/rpc.rs:31:34}}
```


