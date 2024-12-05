# Handling RPC requests

Let's connect `dchatd` up to `JSON-RPC` using DarkFi's [rpc
module](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/rpc).

We'll need to implement a trait called `RequestHandler` for our `Dchat`
struct. `RequestHandler` is an async trait implementing a handler for
incoming JSON-RPC requests. It exposes us to several methods automatically
(including a `pong` response) but it also requires that we implement
two methods: `handle_request` and `connections_mut`.

Let's start with `handle_request`. `handle_request` is simply a
handle for processing incoming JSON-RPC requests that takes a
`JsonRequest` and returns a `JsonResult`. `JsonRequest` is a
`JSON-RPC` request object, and `JsonResult` is an enum that wraps
around a given `JSON-RPC` object type. These types are defined inside
[jsonrpc.rs](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/rpc/jsonrpc.rs).

We'll use `handle_request` to run a match statement on
`JsonRequest.method`.

Running a match on `method` will allow us to branch out to functions
that respond to methods received over `JSON-RPC`.  We haven't implemented
any methods yet, so for now let's just return a `JsonError`.

```rust
#[async_trait]
impl RequestHandler<()> for Dchat {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonError::new(ErrorCode::InvalidRequest, None, req.id).into()
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some(_) | None => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}
```
