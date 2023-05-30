#!/bin/bash

while true; do
    drk ping 2> /dev/null
    if [ $? == 0 ]; then
        break
    fi
    sleep 1
done

drk wallet --initialize
drk scan
drk subscribe blocks

