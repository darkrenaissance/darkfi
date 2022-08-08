# Methods

We're ready to deploy our `JsonRpcInterface`. But right now now it just
returns `JsonError::MethodNotFound`. So before testing out the JSON-RPC,
let's implement some methods.

We'll start with a simple `pong` method that replies to `ping`.

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
