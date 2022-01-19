zkas bincode
============

The bincode design for zkas is the compiled code in the form of a
binary blob, that can be read by a program and fed into the VM.

Our programs consist of three sections: `constant`, `contract`, and
`circuit`. Our bincode represents the same. Additionally, there is
an optional section called `.debug` which can hold debug info related
to the binary.

We currently keep everything on the same stack, so we avoid having to
deal with different types. Instead, we rely that the compiler does
a proper parse and analysis of the source code, so we are sure that
in the VM, when referenced, the types shall be correct.

The compiled binary blob has the following layout:

```
MAGIC_BYTES
BINARY_VERSION
.constant
CONSTANT_TYPE CONSTANT_NAME 
CONSTANT_TYPE CONSTANT_NAME 
...
.contract
WITNESS_TYPE
WITNESS_TYPE
...
.circuit
OPCODE ARG_NUM STACK_INDEX ... STACK_INDEX
OPCODE ARG_NUM STACK_INDEX ... STACK_INDEX
...
.debug
TBD
```

Integers in the binary are encoded using variable-integer encoding.
See [`serial.rs`](../../../src/util/serial.rs) for our Rust
implementation.

## `MAGIC_BYTES`

The magic bytes are the file signature consisting of four bytes used
to identify the zkas binary code. They consist of:

> `0x0b` `0xxx` `0xb1` `0x35`


## `BINARY_VERSION`

The binary code also contains the binary version to allow parsing
potential different formats in the future.

> `0x01`

## `.constant`

The constants in the `.constant` section are declared with their type
and name, so that the VM knows how to search for the builtin constant
and add it to the stack.


## `.contract`

The `.contract` section holds the circuit witness values in the form
of `WITNESS_TYPE`. Their stack index is incremented for each witness
as they're kept in order like in the source file. The witnesses
that are of the same type as the circuit itself (typically `Base`)
will be loaded into the circuit as _private values_ using the Halo2
`load_private` API.


## `.circuit`

The `.circuit` section holds the procedural logic of the ZK proof.
In here we have statements with opcodes that are executed as
understood by the VM. The statements are in the form of:

> `OPCODE ARG_NUM STACK_INDEX ... STACK_INDEX`

where:

|    Element    |                            Description                           |
|---------------|------------------------------------------------------------------|
| `OPCODE`      | The opcode we wish to execute                                    |
| `ARG_NUM`     | The number of arguments given to this opcode                     |
|               | (Note the VM should be checking the correctness of this as well) |
| `STACK_INDEX` | The location of the argument on the stack.                       |
|               | (This is supposed to be repeated `ARG_NUM` times)                |


In case an opcode has a return value, the value shall be pushed to
the stack and become available for later references.

## `.debug`

TBD
