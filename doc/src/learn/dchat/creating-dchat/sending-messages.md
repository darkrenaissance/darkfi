# Sending messages

The core of our application has been built. All that's left is to add a UI
that takes user input, creates a `DchatMsg` and sends it over the network.

Let's start by creating a `send()` function inside `Dchat`. This will
introduce us to a new p2p method that is essential to our chat app:
`p2p.broadcast()`.

```
{{#include ../../../../../example/dchat/src/main.rs:send}}
```

We pass a `String` called msg that will be taken from user input. We use
this input to initialize a message of the type `DchatMsg` that the network
can now support. Finally, we pass the message into `p2p.broadcast()`.
  
Here's what happens under the hood:

```rust
{{#include ../../../../../src/net/p2p.rs:broadcast}}
```

This is pretty straightforward: `broadcast()` takes a generic `Message` type
and sends it across all the channels that our node has access to.

All that's left to do is to create a UI.


