#!/bin/bash
python scripts/parser.py proofs/sapling3.prf | rustfmt > proofs/sapling3.rs
