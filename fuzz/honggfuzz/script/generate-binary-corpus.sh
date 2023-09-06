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

#/usr/bin/env bash
: '
Copies all ZK binary files (.zk.bin) from the main repository into a destination folder
in the fuzzing directory. This allows the compiled example binaries to be used as
test inputs for the fuzzer. This should in turn allow for more efficient fuzzing.
'
set -e

# Run from inside fuzz2 directory
CWD=$(pwd)
DST=$CWD/hfuzz_workspace/zkbinary-decode/input/
cd ..
mkdir -p $DST
find -name "*.zk.bin" -exec cp {} $CWD/hfuzz_workspace/zkbinary-decode/input/ \;
cd $CWD
