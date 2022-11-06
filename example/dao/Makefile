PROOF_SRC = $(shell find proof -type f -name '*.zk')
PROOF_BIN = $(PROOF_SRC:=.bin)

ZKAS = ../../zkas

dao: $(PROOF_BIN)
	RUST_LOG=debug cargo run --release

proof/%.zk.bin: proof/%.zk
	$(ZKAS) $<

clean:
	rm -f $(PROOF_BIN)
