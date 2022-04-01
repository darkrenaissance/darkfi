sraft
=====

Simple Raft consensus implementation.

```
$ cargo build --example peer
$ ./target/debug/examples/peer -p 127.0.0.1:13002 -p 127.0.0.1:13003 -i 1 -l 127.0.0.1:13001
$ ./target/debug/examples/peer -p 127.0.0.1:13001 -p 127.0.0.1:13001 -i 2 -l 127.0.0.1:13002
$ ./target/debug/examples/peer -p 127.0.0.1:13001 -p 127.0.0.1:13002 -i 3 -l 127.0.0.1:13003
```

Try stopping and starting certain nodes to see how new leaders are
elected.
