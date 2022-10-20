# Accept addr

To deploy the `JsonRpcInterface` and start receiving JSON-RPC requests,
we'll need to configure a JSON-RPC accept address.

Let's return to our functions `alice()` and `bob()`. To enable Alice and
Bob to connect to JSON-RPC, we'll need to generalize this return a RPC
`Url` as well as a `Settings`.

Let's define a new struct called `AppSettings` that has two fields,
`Url` and `Settings`.

```rust
{{#include ../../../../../example/dchat/src/main.rs:app_settings}}
```

Next, we'll change our `alice()` method to return a `AppSettings`
instead of a `Settings`.

```rust
{{#include ../../../../../example/dchat/src/main.rs:alice}}
```

And the same for `bob()`:

```rust
{{#include ../../../../../example/dchat/src/main.rs:bob}}
```

Update `main()` with the new type:

```rust
#[async_std::main]
async fn main() -> Result<()> {
    let settings: Result<AppSettings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => alice(),
            "b" => bob(),
            _ => Err(ErrorMissingSpecifier.into()),
        },
        None => Err(ErrorMissingSpecifier.into()),
    };

    let settings = settings?.clone();

    let p2p = net::P2p::new(settings.net).await;
    //...
    }
}
```
