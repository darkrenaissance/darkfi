zkas
====

zkas is a compiler for the Halo2 zkVM language used in
[DarkFi](https://codeberg.org/darkrenaissance/darkfi).

The current implementation found in the DarkFi repository inside
[`src/zkas`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/zkas)
is the reference compiler and language implementation. It is a
toolchain consisting of a lexer, parser, static and semantic analyzers,
and a binary code compiler.

The
[`main.rs`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/bin/zkas/src/main.rs)
file shows how this toolchain is put together to produce binary code
from source code.

# Architecture

The main part of the compilation happens inside the parser. New opcodes
can be added by extending
[`opcode.rs`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/zkas/opcode.rs).

```rust
{{#include ../../../bin/zkas/src/main.rs:zkas}}
```

