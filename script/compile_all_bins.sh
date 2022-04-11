#!/bin/sh
bins="$(find bin -maxdepth 1 | tail -n+2 | sed 's,bin/,,' | tr '\n' ' ' |  sed 's,tau,,')"
bins_tau="$(find bin/tau -maxdepth 1 | sed 's,bin/tau/,,' | grep -w 'taud\|tau-cli' | tr '\n' ' ')"
make BINS="$bins_tau $bins"
