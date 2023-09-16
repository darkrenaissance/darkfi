#/usr/bin/env bash
: '
Copies all ZK binary files (.zk.bin) from the main repository into a destination folder
in the fuzzing directory. This allows the compiled example binaries to be used as
test inputs for the fuzzer. This should in turn allow for more efficient fuzzing.
'
set -e

# Run from inside fuzz/honggfuzz/ directory
CWD=$(pwd)
DST=$CWD/hfuzz_workspace/zkas-compile/input/
cd ../..
mkdir -p $DST
find -name "*.zk" -exec cp {} $CWD/hfuzz_workspace/zkas-compile/input/ \;
cd $CWD
