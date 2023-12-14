# Sending messages

The core of our application has been built. All that's left is to make a
python command-line tool that takes user input and sends it to `dchatd`
over `JSON-RPC`. 

We'll implement a `JSON-RPC` method called `send` that takes some user
data. When `dchatd` receives `send` it will create a `DchatMsg` and send
it over the network using `p2p.broadcast`.

This is `p2p.broadcast`:

```rust
/// Broadcasts a message concurrently across all active channels.
pub async fn broadcast<M: Message>(&self, message: &M) {
    self.broadcast_with_exclude(message, &[]).await
}
```

`broadcast` takes a generic `Message` type and sends it across all the
channels that our node has access to.

All that's left to do is to create a python command-line tool with
`JSON-RPC` integration.
