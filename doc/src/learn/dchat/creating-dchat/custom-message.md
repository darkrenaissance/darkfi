### Creating a custom Message type

We'll start by creating a custom Message type called Dchatmsg. This is the
data structure that we'll use to send messages between dchat instances.

Messages on the p2p network must implement the Message trait. Message is a
generic type that standardizes all messages on DarkFi's p2p network.

We define a custom type called Dchatmsg that implements the Message
trait. We also add serde's SerialEncodable and SerialDecodable to our
struct definition so our messages can be parsed by the network.

The Message trait requires that we implement a method called name(),
which returns a str of the struct's name.

```
use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
};

impl net::Message for Dchatmsg {
    fn name() -> &'static str {
        "Dchatmsg"
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Dchatmsg {
    pub msg: String,
}
```

For the purposes of our chat program, we will also define a buffer where
we can write messages upon receiving them on the p2p network. We'll wrap
this in a Mutex to ensure thread safety and an Arc pointer so we can
pass it around.

```
use async_std::sync::{Arc, Mutex};

pub type DchatmsgsBuffer = Arc<Mutex<Vec<Dchatmsg>>>;
```


