#!/bin/bash -x
cd ../..
python script/zkas.py proof/mint.zk --bincode
du -sh proof/mint.zk.bin
python script/zkas.py proof/mint.zk
#python script/zkas.py proof/mint.zk
cd example/halo2/
cargo run --release --bin vm2

