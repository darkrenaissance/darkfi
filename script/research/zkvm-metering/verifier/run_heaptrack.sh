#!/bin/bash

SRC_DIR="../generator/proof"

for file in "$SRC_DIR"/*.zk; do
    filename=$(basename "$file")
    name="${filename%.zk}"

    heaptrack --output "output/${name}" ./verifier "$name"
done
