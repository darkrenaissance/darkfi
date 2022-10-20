# Slap on a UI

We'll create a new method called `menu()` inside the `Dchat`
implementation. It implements a highly simple UI that allows a user to
send messages and see received messages inside the inbox. Our inbox
simply displays the messages that `ProtocolDchat` has saved in the
`DchatMsgBuffer`.

Here's what it should look like:

```rust
{{#include ../../../../../example/dchat/src/main.rs:menu}}
```

We'll call `menu()` inside of `dchat::start()` along with our other methods, like so:

```rust
    async fn start(&mut self, ex: Arc<Executor<'_>>) -> Result<()> {
        let ex2 = ex.clone();

        self.register_protocol(self.recv_msgs.clone()).await?;
        self.p2p.clone().start(ex.clone()).await?;
        self.p2p.clone().run(ex.clone()).await?;

        self.menu().await?;

        self.p2p.stop().await;
        Ok(())
    }
```

But wait- if you try running this code, you'll notice that the menu never
gets displayed. That's because we call `.await` on the previous function
call, `p2p.run()`. `p2p.run()` is a loop that runs until we exit the program,
so in order for it to not block other threads from executing we'll need
to detach it in the background.

The complete implementaion looks like this:

```rust
{{#include ../../../../../example/dchat/src/main.rs:start}}
```
