/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#!/usr/bin/env python3

"""
Take a binary file as input and prints its bytes as ordinals, formatted as an
array. The goal is to take a file that has caused a crash during binary fuzzing
and convert it into a unit test that can detect panic regressioins.
See darkfi/src/zkas/decoder.rs for example unit tests.

input:
    - binary.bin

output
    - [1, 2, 3, 5, 8, 11]

Now the output can be easily pasted into a unit test. 
"""
import os.path
import sys

if len(sys.argv) != 2:
    print(f"Usage: {__file__} <binary_file>")
    exit(1)

if not os.path.isfile(sys.argv[1]):
    print("Argument is not a file")
    exit(2)

bytes = []
with open(sys.argv[1], "rb") as f:
    while (byte := f.read(1)):
        bytes.append(str(ord(byte)))

print(f"[{', '.join(bytes)}]")
