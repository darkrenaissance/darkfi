# Glossary

* Pedersen commitment: a hiding and binding commitment scheme that
  takes a value and produces an elliptic curve point representing the
  commitment
	* [Explainer](https://medium.com/coinmonks/zero-knowledge-proofs-um-what-a092f0ee9f28)
* zkas: the programming language in which you can write zk circuits
	* [zkas compiler](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/zkas)
        * [An example circuit written in zkas](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/example/simple.zk)
* zkvm: the virtual machine in which zkas circuit binary is run
	* it runs during proof generation
		* [Rust example](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/tests/zkvm_opcodes.rs)
		* [Python example](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/bin/zkrunner/zkrunner.py#L141-L160)
* zkrunner: a Python script which allows you, instead of providing
  circuit, witness, public inputs and code to generate/verify proof,
  you provide circuit, witness
	* [Example](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/bin/zkrunner/zkrunner.py#L180)
