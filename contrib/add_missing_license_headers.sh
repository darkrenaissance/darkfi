#!/bin/sh

files="$(find . -type f -name '*.rs' | grep -v 'target/')"

echo "$files" | while read -r file ; do
	if ! grep -q '/* This file is part of DarkFi ' "$file"; then
		tmp="$(mktemp)"
		cat contrib/license.header "$file" > "$tmp"
		mv -v "$tmp" "$file"
	fi
done
