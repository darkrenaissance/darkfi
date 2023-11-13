#!/usr/bin/env sh


: ' 
Requirements: `exiftool`

This script makes use of exiftool to remove metadata from a file.
Undesirable metadata might include author, operating system,
version of image editor used to create an image, geolocation, etc.
Often this information is present in files without our knowledge
or assent so it is a good idea to proactively remove it using
this script of exiftool directly.

Exiftool works well for removing metadata from images. However,
it cannot completely remove metadata from PDFs. See the exiftool
docs for more information.

This script should be run before a file is committed to
the repository. 

It can be run non-destructively as exiftool will create a copy 
of the original file and preserve it with the string "_original" 
appended to the end.
e.g. `test.jpg` as input to exiftool will create `test.jpg_original`.

This script is simple and removes metadata only from a single file.
However, it is absolutely possible to use exitool in conjunction with
`find` and a `for` loop to remove metadata, either using this tool
or with exiftool directly.
'

set -e

if [ "$#" -ne 1 ]; then
	echo "Usage: $0 <file>"
	exit 1
fi


FILE="$1"

if ! [ -f $FILE ]; then
	echo "Target should be a file"
	exit 2
else
	exiftool -all= $FILE
fi

