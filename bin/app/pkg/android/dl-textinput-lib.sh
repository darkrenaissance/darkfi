#!/bin/bash
# Download GameTextInput library from Android Maven repository
# This script downloads and extracts the GameTextInput headers and libraries

set -e

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "$SCRIPT_DIR/../.." && pwd)

LIBS_DIR=$SCRIPT_DIR/libs
INCLUDE_DIR=$PROJECT_ROOT/src/android/textinput/include

VERSION=4.0.0
AAR=games-text-input-$VERSION.aar
URL=https://dl.google.com/android/maven2/androidx/games/games-text-input/$VERSION/$AAR
TMPDIR=/tmp/games-text-input-$VERSION

cleanup() {
    rm -rf $TMPDIR
}
trap cleanup EXIT

# Clean existing files
rm -rf $LIBS_DIR
rm -rf $INCLUDE_DIR/game-text-input

# Download AAR
mkdir -p $TMPDIR
cd $TMPDIR
wget $URL
unzip $AAR
# Copy libs
mv prefab/modules/game-text-input/libs $LIBS_DIR/
# Copy headers
mkdir -p $INCLUDE_DIR/game-text-input
mv prefab/modules/game-text-input/include/* $INCLUDE_DIR/game-text-input/

echo "GameTextInput ${GAMETEXTINPUT_VERSION} installation complete!"
echo "  Libraries: $LIBS_DIR"
echo "  Headers:  $INCLUDE_DIR"

