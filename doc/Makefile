.POSIX:

DARKFID_JSONRPC = src/clients/darkfid_jsonrpc.md
CASHIERD_JSONRPC = src/clients/cashierd_jsonrpc.md
FAUCETD_JSONRPC = src/clients/faucetd_jsonrpc.md

JSONRPC = \
	$(DARKFID_JSONRPC) \
	$(CASHIERD_JSONRPC) \
	$(FAUCETD_JSONRPC)

all: $(JSONRPC)
	mdbook build
	cp -f theme/logo* book/html/
	$(MAKE) -C ../ rustdoc
	cp -r ../target/doc/* book/html/rustdoc/

$(DARKFID_JSONRPC):
	@echo "# darkfid JSON-RPC API" > $@
	@for i in blockchain tx wallet misc; do \
		echo "## $$i methods" >> $@ ;\
		./build_jsonrpc.py ../bin/darkfid/src/rpc_$$i.rs >> $@ ;\
	done

$(CASHIERD_JSONRPC):
	@echo "# cashierd JSON-RPC API" > $@
	@./build_jsonrpc.py ../bin/cashierd/src/main.rs >> $@

$(FAUCETD_JSONRPC):
	@echo "# faucetd JSON-RPC API" > $@
	@./build_jsonrpc.py ../bin/faucetd/src/main.rs >> $@

clean:
	rm -rf book

github: all
	ghp-import -m "Generate mdbook" -b gh-pages book/html

.PHONY: $(JSONRPC) all clean github
