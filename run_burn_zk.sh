#!/bin/bash -x
python script/zkas.py proof/burn.zk --bincode
du -sh proof/burn.zk.bin
python script/zkas.py proof/burn.zk
#python script/zkas.py proof/mint.zk
cargo run --release --bin vm2_burn

