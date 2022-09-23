zkas bincode
============

The bincode design for zkas is the compiled code in the form of a
binary blob, that can be read by a program and fed into the VM.

Our programs consist of four sections: `constant`, `literal`,
`contract`, and `circuit`. Our bincode represents the
same. Additionally, there is an optional section called `.debug`
which can hold debug info related to the binary.

We currently keep all variables on one stack, and literals on another
stack. Therefore before each `STACK_INDEX` we prepend `STACK_TYPE` so
the VM is able to know which stack it should do lookup from.

The compiled binary blob has the following layout:

```
MAGIC_BYTES
BINARY_VERSION
.constant
CONSTANT_TYPE CONSTANT_NAME 
CONSTANT_TYPE CONSTANT_NAME 
...
.literal
LITERAL
LITERAL
...
.contract
WITNESS_TYPE
WITNESS_TYPE
...
.circuit
OPCODE ARG_NUM STACK_TYPE STACK_INDEX ... STACK_TYPE STACK_INDEX
OPCODE ARG_NUM STACK_TYPE STACK_INDEX ... STACK_TYPE STACK_INDEX
...
.debug
TBD
```

Integers in the binary are encoded using variable-integer encoding.
See [`serial.rs`](https://github.com/darkrenaissance/darkfi/blob/master/src/util/serial.rs)
for our Rust implementation.

## Sections

### `MAGIC_BYTES`

The magic bytes are the file signature consisting of four bytes used
to identify the zkas binary code. They consist of:

> `0x0b` `0x01` `0xb1` `0x35`


### `BINARY_VERSION`

The binary code also contains the binary version to allow parsing
potential different formats in the future.

> `0x02`

### `.constant`

The constants in the `.constant` section are declared with their type
and name, so that the VM knows how to search for the builtin constant
and add it to the stack.

### `.literal`

The literals in the `.literal` section are currently unsigned integers
that get parsed into a `u64` type inside the VM. In the future this
could be extended with signed integers, and strings.


### `.contract`

The `.contract` section holds the circuit witness values in the form
of `WITNESS_TYPE`. Their stack index is incremented for each witness
as they're kept in order like in the source file. The witnesses
that are of the same type as the circuit itself (typically `Base`)
will be loaded into the circuit as _private values_ using the Halo2
`load_private` API.


### `.circuit`

The `.circuit` section holds the procedural logic of the ZK proof.
In here we have statements with opcodes that are executed as
understood by the VM. The statements are in the form of:

> `OPCODE ARG_NUM STACK_TYPE STACK_INDEX ... STACK_TYPE STACK_INDEX`

where:

|    Element    |                            Description                           |
|---------------|------------------------------------------------------------------|
| `OPCODE`      | The opcode we wish to execute                                    |
| `ARG_NUM`     | The number of arguments given to this opcode                     |
|               | (Note the VM should be checking the correctness of this as well) |
| `STACK_TYPE`  | Type of the stack to do lookup from (variables or literals)      |
|               | (This is prepended to every `STACK_INDEX`)                       |
| `STACK_INDEX` | The location of the argument on the stack.                       |
|               | (This is supposed to be repeated `ARG_NUM` times)                |


In case an opcode has a return value, the value shall be pushed to
the stack and become available for later references.

### `.debug`

TBD

## Syntax Reference

### Variable Types

| Type               | Description                                    |
| ------------------ | ---------------------------------------------- |
| `EcPoint`          | Elliptic Curve Point.                          |
| `EcFixedPoint`     | Elliptic Curve Point (constant).               |
| `EcFixedPointBase` | Elliptic Curve Point in Base Field (constant). |
| `Base`             | Base Field Element.                            |
| `BaseArray`        | Base Field Element Array.                      |
| `Scalar`           | Scalar Field Element.                          |
| `ScalarArray`      | Scalar Field Element Array.                    |
| `MerklePath`       | Merkle Tree Path.                              |
| `Uint32`           | Unsigned 32 Bit Integer.                       |
| `Uint64`           | Unsigned 64 Bit Integer.                       |

### Literal Types

| Type               | Description                                    |
| ------------------ | ---------------------------------------------- |
| `Uint64`           | Unsigned 64 Bit Integer.

### Opcodes

| Opcode               | Description                                                     |
| -------------------- | --------------------------------------------------------------- |
| `EcAdd`              | Elliptic Curve Addition.                                        |
| `EcMul`              | Elliptic Curve Multiplication.                                  |
| `EcMulBase`          | Elliptic Curve Multiplication with `Base`.                      |
| `EcMulShort`         | Elliptic Curve Multiplication with a u64 wrapped in a `Scalar`. |
| `EcGetX`             | Get X Coordinate of Elliptic Curve Point.                       |
| `EcGetY`             | Get Y Coordinate of Elliptic Curve Point.                       |
| `PoseidonHash`       | Poseidon Hash of N Elements.                                    |
| `MerkleRoot`         | Compute a Merkle Root.                                          |
| `BaseAdd`            | `Base` Addition.                                                |
| `BaseMul`            | `Base` Multiplication.                                          |
| `BaseSub`            | `Base` Subtraction.                                             |
| `WitnessBase`        | Witness an unsigned integer into a `Base`.                      |
| `RangeCheck`         | Perform a (either 64bit or 253bit) range check over some `Base` |
| `LessThan`           | Compare if `Base` a is lesser than `Base` b                     |
| `ConstrainInstance`  | Constrain a `Base` to a Circuit's Public Input.                 |

### Built-in Opcode Wrappers

| Opcode                | Function                                                | Return        |
| --------------------- | ------------------------------------------------------- | ------------- |
| `EcAdd`               | `ec_add(EcPoint a, EcPoint b)`                          | `(EcPoint c)` |
| `EcMul`               | `ec_mul(EcPoint a, EcPoint c)`                          | `(EcPoint c)` |
| `EcMulBase`           | `ec_mul_base(Base a, EcFixedPointBase b)`               | `(EcPoint c)` |
| `EcMulShort`          | `ec_mul_short(Base a, EcFixedPointShort b)`             | `(EcPoint c)` |
| `EcGetX`              | `ec_get_x(EcPoint a)`                                   | `(Base x)`    |
| `EcGetY`              | `ec_get_y(EcPoint a)`                                   | `(Base y)`    |
| `PoseidonHash`        | `poseidon_hash(Base a, ..., Base n)`                    | `(Base h)`    |
| `MerkleRoot`          | `merkle_root(Uint32 i, MerklePath p, Base a)`           | `(Base r)`    |
| `BaseAdd`             | `base_add(Base a, Base b)`                              | `(Base c)`    |
| `BaseMul`             | `base_mul(Base a, Base b)`                              | `(Base c)`    |
| `BaseSub`             | `base_sub(Base a, Base b)`                              | `(Base c)`    |
| `WitnessBase`         | `witness_base(123)`                                     | `(Base a)`    |
| `RangeCheck`          | `range_check(64, Base a)`                               | `()`          |
| `LessThan`            | `less_than(Base a, Base b)`                             | `()`          |
| `ConstrainInstance`   | `constrain_instance(Base a)`                            | `()`          |

## Decoding the bincode

An example decoder implementation can be found in zkas'
[`decoder.rs`](https://github.com/darkrenaissance/darkfi/blob/master/src/zkas/decoder.rs)
module.
