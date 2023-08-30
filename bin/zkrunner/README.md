zkrunner
========

`zkrunner` is a simple Python script using the DarkFi SDK Python
bindings providing a CLI for prototyping zkas proofs.

## Usage

Refer to the [README.md of the python bindings](../../src/sdk/python/README.md)
to see how to install and use them. They're necessary for zkrunner to
work properly.

Help text:

```
$ ./zkrunner.py -h
```

Running a demo:

```
$ ./witness_gen.py | ./zkrunner.py -w - opcodes.zk
```

The program expects a path to a `witness.json` file containing the
information about witnesses and public inputs for the proof, and a
path to a zkas circuit source code (does not have to be compiled).
The witnesses can also be passed via `stdin`.

Once executed, zkrunner will attempt to create and verify the proof.

## Creating witnesses

Refer to the `witness_gen.py` file.
