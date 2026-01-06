zkas bincode
============

The bincode design for zkas is the compiled code in the form of a
binary blob, that can be read by a program and fed into the VM.

Our programs consist of four sections: `constant`, `literal`,
`witness`, and `circuit`. Our bincode represents the
same. Additionally, there is an optional section called `.debug`
which can hold debug info related to the binary.

We currently keep all variables on one heap, and literals on another
heap. Therefore before each `HEAP_INDEX` we prepend `HEAP_TYPE` so
the VM is able to know which heap it should do lookup from.

The compiled binary blob has the following layout:

```
MAGIC_BYTES
BINARY_VERSION
K
NAMESPACE
.constant
CONSTANT_TYPE CONSTANT_NAME 
CONSTANT_TYPE CONSTANT_NAME 
...
.literal
LITERAL_TYPE LITERAL_VALUE
LITERAL_TYPE LITERAL_VALUE
...
.witness
WITNESS_TYPE
WITNESS_TYPE
...
.circuit
OPCODE ARG_NUM HEAP_TYPE HEAP_INDEX ... HEAP_TYPE HEAP_INDEX
OPCODE ARG_NUM HEAP_TYPE HEAP_INDEX ... HEAP_TYPE HEAP_INDEX
...
.debug (optional)
NUM_OPCODES [LINE COLUMN] ...
HEAP_SIZE [HEAP_NAME] ...
NUM_LITERALS [LITERAL_NAME] ...
```

Integers in the binary are encoded using variable-integer encoding.
See the [`serial`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/serial/src/lib.rs)
crate and module for our Rust implementation.

## Sections

### `MAGIC_BYTES`

The magic bytes are the file signature consisting of four bytes used
to identify the zkas binary code. They consist of:

> `0x0b` `0x01` `0xb1` `0x35`


### `BINARY_VERSION`

The binary code also contains the binary version to allow parsing
potential different formats in the future.

> `0x02`

### `K`

This is a 32bit unsigned integer that represents the `k` parameter
needed to know how many rows our circuit needs.

### `NAMESPACE`

This sector after `MAGIC_BYTES`, `BINARY_VERSION`, and `K` contains
the reference namespace of the code. This is the namespace used in
the source code, e.g.:

```
constant "MyNamespace" { ... }
witness  "MyNamespace" { ... }
circuit  "MyNamespace" { ... }
```

The string is serialized with variable-integer encoding.

### `.constant`

The constants in the `.constant` section are declared with their type
and name, so that the VM knows how to search for the builtin constant
and add it to the heap.

### `.literal`

The literals in the `.literal` section are currently unsigned integers
that get parsed into a `u64` type inside the VM. In the future this
could be extended with signed integers, and strings.


### `.witness`

The `.witness` section holds the circuit witness values in the form
of `WITNESS_TYPE`. Their heap index is incremented for each witness
as they're kept in order like in the source file. The witnesses
that are of the same type as the circuit itself (typically `Base`)
will be loaded into the circuit as _private values_ using the Halo2
`load_private` API.


### `.circuit`

The `.circuit` section holds the procedural logic of the ZK proof.
In here we have statements with opcodes that are executed as
understood by the VM. The statements are in the form of:

> `OPCODE ARG_NUM HEAP_TYPE HEAP_INDEX ... HEAP_TYPE HEAP_INDEX`

where:

|    Element    |                            Description                           |
|---------------|------------------------------------------------------------------|
| `OPCODE`      | The opcode we wish to execute                                    |
| `ARG_NUM`     | The number of arguments given to this opcode                     |
|               | (Note the VM should be checking the correctness of this as well) |
| `HEAP_TYPE`   | Type of the heap to do lookup from (variables or literals)       |
|               | (This is prepended to every `HEAP_INDEX`)                        |
| `HEAP_INDEX`  | The location of the argument on the heap.                        |
|               | (This is supposed to be repeated `ARG_NUM` times)                |


In case an opcode has a return value, the value shall be pushed to
the heap and become available for later references.

### `.debug`

The `.debug` section is optional and contains debug information that
maps the compiled binary back to the original source code. This is
useful for debugging circuit failures when only the compiled binary
is available.

> `NUM_OPCODES [LINE COLUMN] ... HEAP_SIZE [HEAP_NAME] ... NUM_LITERALS [LITERAL_NAME] ...`

where:

|    Element      |                            Description                           |
|-----------------|------------------------------------------------------------------|
| `NUM_OPCODES`   | Number of opcodes in the `.circuit` section (VarInt)             |
| `LINE`          | Source line number for this opcode (VarInt)                      |
| `COLUMN`        | Source column number for this opcode (VarInt)                    |
| `HEAP_SIZE`     | Total number of entries on the heap (VarInt)                     |
| `HEAP_NAME`     | Variable name for this heap entry (String)                       |
| `NUM_LITERALS`  | Number of literals in the `.literal` section (VarInt)            |
| `LITERAL_NAME`  | The literal value as a string, e.g., "42" (String)               |

The heap names are serialized in heap order: constants first, then
witnesses, then assigned variables from circuit statements. This
ordering matches the order in which items are pushed onto the heap
during compilation.

Using the debug info, a debugger or tracing tool can display output
such as:

```
Line   Source       Opcode                 Variable             Value
0      L23:C5       EcMulShort             token_commit         [0x3a2f..., 0x91bc...]
1      L24:C5       EcMulBase              rcpt_commit          [0x7d1e..., 0x44fa...]
2      L25:C5       EcAdd                  commitment           [0x8b3c..., 0x22de...]
3      L26:C5       ConstrainInstance      -                    -
```

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
| `LessThanStrict`     | Strictly compare if `Base` a is lesser than `Base` b            |
| `LessThanLoose`      | Loosely compare if `Base` a is lesser than `Base` b             |
| `BoolCheck`          | Enforce that a `Base` fits in a boolean value (either 0 or 1)   |
| `CondSelect`         | Select either `a` or `b` based on if `cond` is 0 or 1           |
| `ZeroCondSelect`     | Output `a` if `a` is zero, or `b` if a is not zero              |
| `ConstrainEqualBase` | Constrain equality of two `Base` elements from the heap         |
| `ConstrainEqualPoint`| Constrain equality of two `EcPoint` elements from the heap      |
| `ConstrainInstance`  | Constrain a `Base` to a Circuit's Public Input.                 |

### Built-in Opcode Wrappers

| Opcode                | Function                                                | Return        |
| --------------------- | ------------------------------------------------------- | ------------- |
| `EcAdd`               | `ec_add(EcPoint a, EcPoint b)`                          | `(EcPoint)`   |
| `EcMul`               | `ec_mul(EcPoint a, EcPoint c)`                          | `(EcPoint)`   |
| `EcMulBase`           | `ec_mul_base(Base a, EcFixedPointBase b)`               | `(EcPoint)`   |
| `EcMulShort`          | `ec_mul_short(Base a, EcFixedPointShort b)`             | `(EcPoint)`   |
| `EcMulVarBase`        | `ec_mul_var_base(Base a, EcNiPoint)`                    | `(EcPoint)`   |
| `EcGetX`              | `ec_get_x(EcPoint a)`                                   | `(Base)`      |
| `EcGetY`              | `ec_get_y(EcPoint a)`                                   | `(Base)`      |
| `PoseidonHash`        | `poseidon_hash(Base a, ..., Base n)`                    | `(Base)`      |
| `MerkleRoot`          | `merkle_root(Uint32 i, MerklePath p, Base a)`           | `(Base)`      |
| `BaseAdd`             | `base_add(Base a, Base b)`                              | `(Base)`      |
| `BaseMul`             | `base_mul(Base a, Base b)`                              | `(Base)`      |
| `BaseSub`             | `base_sub(Base a, Base b)`                              | `(Base)`      |
| `WitnessBase`         | `witness_base(123)`                                     | `(Base)`      |
| `RangeCheck`          | `range_check(64, Base a)`                               | `()`          |
| `LessThanStrict`      | `less_than_strict(Base a, Base b)`                      | `()`          |
| `LessThanLoose`       | `less_than_loose(Base a, Base b)`                       | `()`          |
| `BoolCheck`           | `bool_check(Base a)`                                    | `()`          |
| `CondSelect`          | `cond_select(Base cond, Base a, Base b)`                | `(Base)`      |
| `ZeroCondSelect`      | `zero_cond(Base a, Base b)`                             | `(Base)`      |
| `ConstrainEqualBase`  | `constrain_equal_base(Base a, Base b)`                  | `()`          |
| `ConstrainEqualPoint` | `constrain_equal_point(EcPoint a, EcPoint b)`           | `()`          |
| `ConstrainInstance`   | `constrain_instance(Base a)`                            | `()`          |

## Decoding the bincode

An example decoder implementation can be found in zkas'
[`decoder.rs`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/zkas/decoder.rs)
module.
