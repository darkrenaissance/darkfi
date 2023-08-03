# What is this?

`zkrunner` is a simple Python script that calls into the Darkfi Python SDK.
The Python SDK provides APIs such as creating a circuit, assigning the witness to the circuit and more.

`zkrunner` uses the Python SDK to create a developement environment for zkas developer where:
* the ZKAS developer provides the ZKAS binary code
* the ZKAS developer provides the witness and assigns it accordingly
* zkrunner:
	* sets up the circuit from the binary
	* generates both proving and verifying key
	* creates the proof from the witness and proving key
	* creates the public inputs from the witness
	* verifies the proof using the public inputs and verifying key
* zkrunner times each step as a basic performance benchmark

This is so developers have an easier time to test their zkas circuit.

# Installation

Follow the guide in src/sdk/python/README.md to install the Python bindings and virtual environment.

# Getting Started

* Compile the ZKAS source to ZKAS binary
```
cd <darkmap>
zkas proof/set_v1.zk
```
* Open up `zkrunner.py`, read over the TODOs and comments, provide the path to zkas binary code, witness and assign accordingly.
* After installing Python bindings in your Python installation, simply run `python zkrunner.py [--verbose]`.

# Notes

* "witness" and "witnesses" are used interchangablely.
