#!/usr/bin/env python3
# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2026 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Python tool to render zkVM circuit layouts given zkas source code.
"""
import sys
from darkfi_sdk.zkas import ZkBinary, ZkCircuit
from zkrunner import eprint, load_circuit_witness

def main(witness_file, source_file, output, width, height, font_size):
    print("Compiling zkas code...")
    with open(source_file, "r", encoding="utf-8") as zkas_file:
        zkas_source = zkas_file.read()

    zkbin = ZkBinary(source_file, zkas_source)
    circuit = ZkCircuit(zkbin)
    print("Decoding witnesses...")
    load_circuit_witness(circuit, witness_file)
    circuit = circuit.prover_build()

    if not circuit.render(zkbin.k(), output, width, height, font_size):
        eprint("Rendering failed")
    print(f"Written output to '{output}'")

if __name__ == "__main__":
    from argparse import ArgumentParser

    parser = ArgumentParser(
        prog="zkrender",
        description="Python util for rendering zk circuits",
        epilog="This tool is only for prototyping purposes",
    )
    parser.add_argument(
        "SOURCE",
        help="Path to zkas source code",
    )
    parser.add_argument(
        "-w",
        "--witness",
        required=True,
        help="Path to JSON file holding witnesses",
    )
    parser.add_argument(
        "OUTPUT",
        help="Path to output image",
    )

    parser.add_argument(
        "-W", "--width", type=int,
        default=800,
        help="Image width",
    )
    parser.add_argument(
        "-H", "--height", type=int,
        default=600,
        help="Image width",
    )
    parser.add_argument(
        "-f", "--font-size", type=int,
        default=20,
        help="Image width",
    )

    args = parser.parse_args()
    sys.exit(main(args.witness, args.SOURCE, args.OUTPUT,
                  args.width, args.height, args.font_size))

