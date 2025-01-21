#!/bin/bash

# Download and install appimagetool into your PATH
# https://github.com/AppImage/appimagetool/releases/tag/continuous

APPDIR=DarkFi.AppDir/
rm -fr $APPDIR
mkdir $APPDIR
cp darkfi.desktop $APPDIR

cp ../../data/res/mipmap-xxxhdpi/ic_launcher.png $APPDIR/darkfi.png
cp ../../darkwallet $APPDIR/AppRun
cp -r ../../assets $APPDIR
appimagetool $APPDIR

