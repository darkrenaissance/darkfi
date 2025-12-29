.POSIX:

# Compile target for docs
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)

DARKFID_JSONRPC = src/clients/darkfid_jsonrpc.md

JSONRPC = \
	$(DARKFID_JSONRPC)

all: $(JSONRPC)
	./generate_seminar_ics.py --table > src/dev/seminars.md
	mdbook build
	cp -f theme/logo* book/
	./generate_seminar_ics.py --ics
	$(MAKE) -C ../ rustdoc
	cp -r ../target/$(RUST_TARGET)/doc/* book/dev/
	./remove_chapter_nav_js.py

$(DARKFID_JSONRPC):
	@echo "# darkfid JSON-RPC API" > $@
	./build_jsonrpc.py ../bin/darkfid/src/rpc/mod.rs >> $@
	for i in blockchain stratum tx xmr; do \
		echo "## $$i methods" >> $@ ;\
		./build_jsonrpc.py ../bin/darkfid/src/rpc/rpc_$$i.rs >> $@ ;\
	done

clean:
	rm -rf book

github: all
	./ghp_push.py

.PHONY: $(JSONRPC) all clean github
