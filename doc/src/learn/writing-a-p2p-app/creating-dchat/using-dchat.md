### Using dchat

We are finally ready to test our program. Spin up 5 different terminals.

In terminal 1, run lilith.

```
cargo run --dchat
```

In terminal 2, run Alice.

```
cargo run a 
```

In terminal 3, run Bob.

```
cargo run b
```

In terminal 4, display Alice's debug output.

```
multitail -c /tmp/alice.log
```

In terminal 5, display Bob's debug output.

```
multitail -c /tmp/bob.log
```

Now use the UI to send messages between Alice and Bob. We have
successfully implemented a p2p chat program.


