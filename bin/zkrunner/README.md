# What is this?

`zkrunner` is a simple Python script that calls into the Darkfi Python SDK.
The Python SDK provides APIs such as creating a circuit, assigning the witness to the circuit and more.

`zkrunner` uses the Python SDK to create a developement environment for zkas developer where:
* the zkas developer provides the witness and assigns it accordingly
* zkrunner 1) sets up the circuit, 2) creates the proof, 3) creates the public inputs (from the witness) and 4) verifies the proof
* zkrunner times each step as a basic performance benchmark

This is so developers have an easier time to test their zkas circuit.

# Installation

Follow the guide in src/sdk/python/README.md to install the Python bindings and virtual environment.

# Getting Started

* Open up `zkrunner.py`, provide the path to zkas binary code, witness and assign accordingly.
* After installing Python bindings in your Python installation, simply run `python zkrunner.py [--verbose]`.

# Notes

* "witness" and "witnesses" are used interchangablely.
