.POSIX:

DARKFID_JSONRPC = src/clients/darkfid_jsonrpc.md

all:
	echo "# darkfid JSON-RPC API" > $(DARKFID_JSONRPC)
	for i in blockchain tx wallet misc; do \
		echo "## $$i methods" >> $(DARKFID_JSONRPC);\
		./build_jsonrpc.py ../bin/darkfid2/src/rpc_$$i.rs >> $(DARKFID_JSONRPC);\
	done

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
