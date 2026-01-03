#!/bin/bash

SRC_DIR="../generator/src"

for file in "$SRC_DIR"/*/proof/*.zk.bin; do
    filename=$(basename "$file")
    name="${filename%.zk.bin}"

    heaptrack --output "output/${name}" ./verifier "$file"
done
