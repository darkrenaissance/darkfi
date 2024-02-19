# Methods

We're ready to deploy our `JsonRpcInterface`. But right now now it just
returns `JsonError::MethodNotFound`. So before testing out the JSON-RPC,
let's implement some methods.

We'll start with a simple `pong` method that replies to `ping`.

```rust
{{#include ../../../../../example/dchat/src/rpc.rs:pong}}
```

And add it to `handle_request()`:

```rust
        match req.method.as_str() {
            Some("ping") => self.pong(req.id, req.params).await,
            Some(_) | None => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
            }
```
