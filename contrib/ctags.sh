#!/bin/sh
# Generate a tags file for the codebase
set -ex
git ls-files src bin | ctags --links=no --languages=rust -L-
