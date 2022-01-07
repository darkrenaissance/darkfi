zkas
====

zkas is a compiler for the Halo2 zkVM langage used in
[DarkFi](https://github.com/darkrenaissance/darkfi).

The documentation on both the compiler and the language can be found
in the book: https://darkrenaissance.github.io/darkfi/zkas/zkas.html

The current implementation found in the DarkFi repository inside
https://github.com/darkrenaissance/darkfi/tree/master/zkas is the
reference compiler and language implementation. It is a toolchain
consisting of a lexer, parser, static and semantic analyzers, and a
binary code compiler.

The [`main.rs`](https://github.com/darkrenaissance/darkfi/blob/master/zkas/src/main.rs)
file shows how this toolchain is put together to produce binary code
from source code.
