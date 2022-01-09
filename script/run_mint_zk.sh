#!/bin/bash -x
python zkas.py ../proof/mint.zk --bincode
du -sh ../proof/mint.zk.bin
python zkas.py ../proof/mint.zk
cargo run --all-features --release --example vm
