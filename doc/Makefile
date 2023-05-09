.POSIX:

DARKFID_JSONRPC = src/clients/darkfid_jsonrpc.md
FAUCETD_JSONRPC = src/clients/faucetd_jsonrpc.md

JSONRPC = \
	$(DARKFID_JSONRPC) \
	$(FAUCETD_JSONRPC)

all: $(JSONRPC)
	mdbook build
	cp -f theme/logo* book/
	#$(MAKE) -C ../ rustdoc
	cp -r ../target/doc/* book/development/

$(DARKFID_JSONRPC):
	@echo "# darkfid JSON-RPC API" > $@
	for i in blockchain tx wallet misc; do \
		echo "## $$i methods" >> $@ ;\
		./build_jsonrpc.py ../bin/darkfid/src/rpc_$$i.rs >> $@ ;\
	done

$(FAUCETD_JSONRPC):
	@echo "# faucetd JSON-RPC API" > $@
	./build_jsonrpc.py ../bin/faucetd/src/main.rs >> $@

clean:
	rm -rf book

github: all
	ghp-import -m "Generate mdbook" -b gh-pages book/

.PHONY: $(JSONRPC) all clean github
