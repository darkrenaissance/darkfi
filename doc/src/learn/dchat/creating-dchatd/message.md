# Creating a Message type

We'll start by creating a custom `Message` type called `DchatMsg`. This is the
data structure that we'll use to send messages between `dchat` instances.

Messages on the p2p network must implement the `Message` trait. `Message` is a
generic type that standardizes all messages on DarkFi's p2p network.

We define a custom type called `DchatMsg` that implements the
`Message` trait. We also add `darkfi::util::SerialEncodable` and
`darkfi::util::SerialDecodable` macros to our struct definition so our
messages can be parsed by the network.

`Message` requires that we implement a method called `name`, which
returns a `str` of the struct's name.

For the purposes of our chat program, we will also define a buffer where
we can write messages upon receiving them on the p2p network. We'll wrap
this in a `Mutex` to ensure thread safety and an `Arc` pointer so we can
pass it around.

```rust
{{#include ../../../../../example/dchat/dchatd/src/dchatmsg.rs:msg}}
```
