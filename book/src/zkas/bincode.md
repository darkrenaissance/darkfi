Bincode
=======

The bincode design for zkas is the compiled code in the form of a
binary blob, that can be read by a program and fed into the VM.

Our programs consist of three sections: `constant`, `contract`, and
`circuit`. Our bincode represents the same. Additionally, there is an
optional section called `.debug` which can hold debug info related to
the binary.

We currently keep everything on the same stack, so we avoid having to
deal with different types. Instead, we rely that the compiler does a
proper parse and analysis of the source code, so we are sure that in
the VM, when referenced, the types shall be correct.

The compiled binary blob has the following layout:

```
.constant
CONSTANT_TYPE STACK_INDEX CONSTANT_NAME 
CONSTANT_TYPE STACK_INDEX CONSTANT_NAME 
...
.contract
WITNESS_TYPE STACK_INDEX
WITNESS_TYPE STACK_INDEX
...
.circuit
STATEMENT_TYPE OPCODE ARG_NUM STACK_INDEX ... STACK_INDEX
STATEMENT_TYPE OPCODE ARG_NUM STACK_INDEX ... STACK_INDEX
...
.debug
TBD
```

## `.constant`

The constants in the `.constant` section are declared with their type
stack index, and name, so that the VM knows how to search for the
builtin constant and add it to the stack.

## `.contract`

The `.contract` section holds the circuit witness values in the form
of `WITNESS_TYPE` and `STACK_INDEX`. The witnesses that are of the same
type as the circuit itself (typically `Base`) will be loaded into the
circuit as _private values_ using the Halo2 `load_private` API.

## `.circuit`

The `.circuit` section holds the procedural logic of the ZK proof.
In here we have statements with opcodes that are executed as
understood by the VM. The statements are in the form of:

```
STATEMENT_TYPE OPCODE ARG_NUM STACK_INDEX ... STACK_INDEX
```

where:

* `STATEMENT_TYPE` - Where we currently support an assignment, and a
  call without an assignment.
* `OPCODE` - The opcode we wish to execute
* `ARG_NUM` - The number of arguments given to this opcode.
  (Note, the VM should be checking the correctness of this as well
  before executing the opcode)
* `STACK_INDEX` - The location of the argument on the stack. This is
  supposed to be repeated `ARG_NUM` times.

## `.debug`

TBD
