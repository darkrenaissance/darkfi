zkas
====

zkas is a compiler for the Halo2 zkVM language used in
[DarkFi](https://github.com/darkrenaissance/darkfi).

The current implementation found in the DarkFi repository inside
[`src/zkas`](https://github.com/darkrenaissance/darkfi/tree/master/src/zkas)
is the reference compiler and language implementation. It is a
toolchain consisting of a lexer, parser, static and semantic analyzers,
and a binary code compiler.

The
[`main.rs`](https://github.com/darkrenaissance/darkfi/blob/master/bin/zkas/src/main.rs)
file shows how this toolchain is put together to produce binary code
from source code.
