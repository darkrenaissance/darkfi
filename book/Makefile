.POSIX:

all:
	echo "# darkfid JSON-RPC API" > src/clients/darkfid_jsonrpc.md
	./build_jsonrpc.py ../bin/darkfid/src/main.rs \
		>> src/clients/darkfid_jsonrpc.md
	mdbook build

github: all
	ghp-import -m "Generate mdbook" -b gh-pages book/html

.PHONY: all github
