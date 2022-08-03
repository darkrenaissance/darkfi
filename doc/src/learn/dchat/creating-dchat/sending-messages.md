### Sending messages

The core of our application has been built. All that's left is to add a UI
that takes user input, creates a Dchatmsg and sends it over the network.

Let's start by creating a send() function inside Dchat. This will
introduce us to a new p2p method that is essential to our chat app:
p2p.broadcast().

```
async fn send(&self, msg: String) -> Result<()> {
    let dchatmsg = Dchatmsg { msg };
    self.p2p.broadcast(dchatmsg).await?;
    Ok(())
}
```

We pass a String called msg that will be taken from user input. We use
this input to initialize a message of the type Dchatmsg that the network
can now support. Finally, we pass the message into p2p.broadcast().
  
Here's what happens under the hood:

```
pub async fn broadcast<M: Message + Clone>(&self, message: M) -> Result<()> {
    for channel in self.channels.lock().await.values() {
        channel.send(message.clone()).await?;
    }
    Ok(())
}
```

This is pretty straightforward: broadcast() takes a generic Message type
and sends it across all the channels that our node has access to.

All that's left to do is to create a UI.


