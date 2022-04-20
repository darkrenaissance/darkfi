.POSIX:

all:
	echo "# darkfid JSON-RPC API" > src/clients/darkfid_jsonrpc.md
	./build_jsonrpc.py ../bin/darkfid/src/main.rs \
		>> src/clients/darkfid_jsonrpc.md

	echo "# cashierd JSON-RPC API" > src/clients/cashierd_jsonrpc.md
	./build_jsonrpc.py ../bin/cashierd/src/main.rs \
		>> src/clients/cashierd_jsonrpc.md

	echo "# faucetd JSON-RPC API" > src/clients/faucetd_jsonrpc.md
	./build_jsonrpc.py ../bin/faucetd/src/main.rs \
		>> src/clients/faucetd_jsonrpc.md

	mdbook build
	cp -f theme/logo* book/html/

clean:
	rm -rf book

github: all
	ghp-import -m "Generate mdbook" -b gh-pages book/html

.PHONY: all clean github
