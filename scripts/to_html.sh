#!/bin/bash
#nvim -c ":TOhtml" $1
sed -i "s/PreProc { color: #5fd7ff; }/PreProc { color: #8f2722; }/" $1
sed -i "s/Comment { color: #00ffff; }/Comment { color: #0055ff; }/" $1

