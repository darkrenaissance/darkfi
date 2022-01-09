#!/bin/bash -x
python zkas.py ../proof/burn.zk --bincode
du -sh ../proof/burn.zk.bin
python zkas.py ../proof/burn.zk
cargo run --all-features --release --example vm_burn
