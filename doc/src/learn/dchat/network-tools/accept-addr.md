# Accept addr

To deploy the `JsonRpcInterface` and start receiving JSON-RPC requests,
we'll need to configure a JSON-RPC accept address.

Let's return to our functions `alice()` and `bob()`. To enable Alice and
Bob to connect to JSON-RPC, we'll need to generalize this return a RPC
`Url` as well as a `Settings`.

Let's define a new struct called `AppSettings` that has two fields,
`Url` and `Settings`.

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

{{#include ../../../../../example/dchat/src/main.rs:196}}
    //...
{{#include ../../../../../example/dchat/src/main.rs:224}}
```


