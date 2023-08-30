#/usr/bin/env bash

: ' 
This program can be used as a convenient way to remove a username
from files that will be shared with the dev team. An example is
pasting a large log file or stack trace. Usernames can appear here
due to e.g. file paths in the home directory. This may not be
desirable.

In this case, you can run this program on the file you want to
share and it will create a copy of it in the current working
directory. The copy will have the extension ".scrubbed".

e.g. running this program as user `rms` in /tmp/:

```
sh no-dox.sh /etc/passwd
```

creates the file `/tmp/passwd.scrubbed` with the string `rms`
replaced with the string `user`.

NOTE: It is recommended to verify the output for any mistakes.
This script is simple and is likely to miss some edge-cases.
'
set -e

if [ "$#" -ne 1 ]; then
	echo "Usage: $0 <file>"
	exit 1
fi

# Replace the current user's name.
ME=$(whoami)
# Uncomment the following line and change 'changeme' if the user
# running the script is not the same user as the one you are
# trying to conceal.
#ME="changeme"

# The string in $ME will be replaced with this string.
REPLACE="user"

FILE=$1
DST=$(basename $1)

if ! [ -f $FILE ]; then
	echo "Target should be a file"
	exit 2
else
	sed s/$ME/$REPLACE/g "$FILE" > "$(pwd)/$DST.scrubbed"
fi
