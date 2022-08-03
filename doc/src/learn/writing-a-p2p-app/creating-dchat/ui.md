### Slap on a UI

We'll create a new method called menu() inside the Dchat
implementation. It implements a highly simple UI that allows a user to
send messages and see received messages inside the inbox. Our inbox
simply displays the messages that ProtocolDchat has saved in the
DchatmsgBuffer.

Here's what is should look like:

```

use std::io::stdin;

async fn menu(&self) -> Result<()> {
    let mut buffer = String::new();
    let stdin = stdin();
    loop {
        println!(
            "Welcome to dchat.
s: send message
i: inbox
q: quit "
        );
        stdin.read_line(&mut buffer)?;
        // Remove trailing \n
        buffer.pop();
        match buffer.as_str() {
            "q" => return Ok(()),
            "s" => {
                // Remove trailing s
                buffer.pop();
                stdin.read_line(&mut buffer)?;
                match self.send(buffer.clone()).await {
                    Ok(_) => {
                        println!("you sent: {}", buffer);
                    }
                    Err(e) => {
                        println!("send failed for reason: {}", e);
                    }
                }
                buffer.clear();
            }
            "i" => {
                let msgs = self.recv_msgs.lock().await;
                if msgs.is_empty() {
                    println!("inbox is empty")
                } else {
                    println!("received:");
                    for i in msgs.iter() {
                        if !i.msg.is_empty() {
                            println!("{}", i.msg);
                        }
                    }
                }
                buffer.clear();
            }
            _ => {}
        }
    }
}
```

We'll call menu() inside of dchat::start() along with our other methods, like so:

```
async fn start(&self, ex: Arc<Executor<'_>>) -> Result<()> {
    self.register_protocol(self.recv_msgs.clone()).await?;

    self.p2p.clone().start(ex.clone()).await?;
    self.p2p.clone().run(ex.clone()).await?;

    self.menu().await?;

    Ok(())
}
```

But wait- if you try running this code, you'll notice that the menu never
gets displayed. That's because we call .await on the previous function
call, p2p.run(). p2p.run() is a loop that runs until we exit the program,
so in order for it to not block other threads from executing we'll need
to detach it in the background.

The complete implementaion looks like this:

```
async fn start(&mut self, ex: Arc<Executor<'_>>) -> Result<()> {
    let ex2 = ex.clone();

    self.register_protocol(self.recv_msgs.clone()).await?;
    self.p2p.clone().start(ex.clone()).await?;
    ex2.spawn(self.p2p.clone().run(ex.clone())).detach();

    self.menu().await?;

    Ok(())
}
```


