# DarkFi RPC

First, we'll need to connect dchat up to JSON-RPC using DarkFi's [rpc
module](https://github.com/darkrenaissance/darkfi/tree/master/src/rpc).

# AppSettings 

We'll need to set an JSON-RPC `Url` that is specific to our nodes, Alice
and Bob. To do that, let's return to our functions `alice()` and `bob()`
that return the type `Settings`. To enable Alice and Bob to connect to
JSON-RPC, we'll need to generalize this to include a RPC `Url`.

Let's define a new struct called `AppSettings` that has two fields,
a RPC `Url` and `Settings`.

```rust
{{#include ../../../../../example/dchat/src/main.rs:123:132}}
```

Next, we'll change our `alice()` method to return a `AppSettings`
instead of a `Settings`.

```rust
{{#include ../../../../../example/dchat/src/main.rs:135}}
    //...
{{#include ../../../../../example/dchat/src/main.rs:143:158}}
```

And the same for `bob()`:

```rust
{{#include ../../../../../example/dchat/src/main.rs:160}}
    //...
{{#include ../../../../../example/dchat/src/main.rs:170:181}}
```

Update `main()` with the new type:

```rust
{{#include ../../../../../example/dchat/src/main.rs:183:192}}

{{#include ../../../../../example/dchat/src/main.rs:194}}

{{#include ../../../../../example/dchat/src/main.rs:197}}
    //...
{{#include ../../../../../example/dchat/src/main.rs:225}}
```

# JsonRpcInterface

Next, we'll define a new struct called `JsonRpcInterface` that takes
two values, a `Url` that we'll connect the JSON-RPC to, and a pointer
to the p2p network.

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:1:17}}
```

We'll need to implement a trait called `RequestHandler` for
the `JsonRpcInterface`. `RequestHandler` exposes a method called
`handle_request()` which is a handle for processes incoming
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
that handle respective methods.  We haven't implemented any methods yet,
so for now let's just return a `JsonError`.

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:19:28}}
{{#include ../../../../../example/dchat/src/rpc.rs:31:34}}
```

# Listen and serve

Now let's implement some methods. We'll start with a simple `pong`
method that replies to `ping`.

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:36:43}}
{{#include ../../../../../example/dchat/src/rpc.rs:53}}
```

And add it to `handle_request()`:

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:19:21}}
        //...
{{#include ../../../../../example/dchat/src/rpc.rs:28:29}}
{{#include ../../../../../example/dchat/src/rpc.rs:31:34}}
```

To deploy this, we'll need to invoke an `rpc::server` method,
`listen_and_serve()`.  `listen_and_serve()` starts a JSON-RPC server that
is bound to the provided accept URL and uses our previously implemented
`RequestHandler` to handle incoming requests.

