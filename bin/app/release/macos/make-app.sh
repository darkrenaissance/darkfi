#!/bin/bash

APPDIR=darkfi.app/
rm -fr $APPDIR
mkdir $APPDIR
cp -r Contents $APPDIR

cp ../../darkfi-app.macos $APPDIR/darkfi
cp ../../data/res/mipmap-xxxhdpi/ic_launcher.png $APPDIR/darkfi.png
cp -r ../../assets $APPDIR

hdiutil create -volname darkfi -srcfolder $APPDIR -ov -format UDZO darkfi.dmg
#zip -r darkfi.app.zip $APPDIR

