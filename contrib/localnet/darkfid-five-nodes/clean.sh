#!/bin/sh
rm -rf darkfid0 darkfid1 darkfid2 darkfid3 darkfid4
# clean log files
rm /tmp/f_history.log  /tmp/lead_history.log &>/dev/null &
touch /tmp/f_history.log
touch /tmp/lead_history.log


