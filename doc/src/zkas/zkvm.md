zkVM
====

The DarkFi zkVM is a single zkSNARK circuit based on
[Halo2](https://github.com/zcash/halo2) which requires no trusted
setup and is able to execute and prove compiled _zkas_ bincode.

The zkVM is designed in such a way that it's able to choose code paths
based on the bincode and as such, create a zkSNARK proof specific to
the zkas circuit. In this document, we'll explain this machinery from
a high level. A preliminary to understanding the zkVM is to understand
the [zkas bincode](bincode.md) and its layout.

## High-level operation

The entire VM can be thought of as a machine with heap
access to values (variables) that are constructed within
the ZK circuit.  Upon initialization, the VM instantiates
two heaps, of which one holds literals (currently `u64` is
supported), and the other holds arbitrary types defined in
[`HeapVar`](https://dark.fi/book/development/darkfi/zk/vm_heap/enum.HeapVar.html)

Once the heaps are instantiated, the circuit initializes all the
available halo2 gadgets so they're ready for use, and also to create
and have access to any lookup tables.

Next, if there are any constants defined in the `constant` section
of zkas, they are created and pushed to the heap:

```rust,no_run,no_playground
{{#include ../../../src/zk/vm.rs:constant_init}}
```

If all is successful, the VM proceeds with any available literals in
the `circuit` section and pushes them onto the literals heap:

```rust,no_run,no_playground
{{#include ../../../src/zk/vm.rs:literals_init}}
```

At this point, the VM is done with initializing the constants used,
and proceeds with the private witnesses of the ZK proof that are
located in the `witness` section of the zkas bincode. We simply
loop through the witnesses in order, and depending on what they are,
we witness them with specialized halo2 functions:

```rust,no_run,no_playground
{{#include ../../../src/zk/vm.rs:witness_init}}
```

Once this is done, everything is set up and the VM proceeds with
executing the input opcodes that are located in the `circuit` section
of the zkas bincode in a sequential fashion. Opcodes are able to
take a defined number of inputs and are able to optionally produce
a single output. The inputs are referenced from the heap, by index.
The output that can be produced by an opcode is also pushed onto the
heap when created. An example of this operation can be seen within
the following snippet from the zkVM:

```rust,no_run,no_playground
{{#include ../../../src/zk/vm.rs:opcode_begin}}
```

As the opcodes are being executed, the halo2 API lets us return any
possible proof verification error so the verifier is able to know
if the input proof is valid or not. Any possible public inputs to a
circuit are also fed into the `constrain_instance` opcode, so that's
how even public inputs can be enforced in the same uniform fashion
like the rest.
