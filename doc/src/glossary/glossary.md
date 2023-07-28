# Glossary

* Pedersen commitment: a hiding and binding commitment scheme that takes a value and produces an elliptic curve point representing the commitment
	* Explainer: https://medium.com/coinmonks/zero-knowledge-proofs-um-what-a092f0ee9f28
* zkas: the programming language in which you can write zk circuits
	* zkas compiler: https://github.com/darkrenaissance/darkfi/tree/ae9801fce10c1403ac293303b75a15db115b4da6/src/zkas
        * an example circuit written in zkas:  https://github.com/darkrenaissance/darkfi/blob/ae9801fce10c1403ac293303b75a15db115b4da6/example/simple.zk
* zkvm: the virtual machine in which zkas circuit binary is run
	* it runs during proof generation
		* rust example: https://github.com/darkrenaissance/darkfi/blob/ae9801fce10c1403ac293303b75a15db115b4da6/tests/zkvm_opcodes.rs
		* python example: https://github.com/darkrenaissance/darkfi/blob/ae9801fce10c1403ac293303b75a15db115b4da6/bin/zkrunner/zkrunner.py#L141-L160
* zkrunner: a Python script which allows you, instead of providing circuit, witness, public inputs and code to generate/verify proof, you provide circuit, witness
		* example: https://github.com/darkrenaissance/darkfi/blob/master/bin/zkrunner/zkrunner.py#L180
