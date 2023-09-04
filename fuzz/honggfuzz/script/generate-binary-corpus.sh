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
