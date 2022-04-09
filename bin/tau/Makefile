.POSIX:

# Install prefix
PREFIX = ~/.local

# Cargo binary
CARGO = cargo

BINS = taud tau-cli 

BINSDIR = ./bin

BINDEPS = \
	$(shell find ./taud/src -type f) \
	$(shell find ./tau-cli/src -type f) \
	$(shell find . -type f -name taud_config.toml)

all: $(BINS)

$(BINS): $(BINDEPS)
	mkdir -p $(BINSDIR) 
	$(CARGO) build --all-features --release --package $@
	cp -f ../../target/release/$@ $(BINSDIR)/$@

clean: 
	rm -f $(BINSDIR)/*

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BINSDIR)/taud $(DESTDIR)$(PREFIX)/bin
	cp -f $(BINSDIR)/tau-cli $(DESTDIR)$(PREFIX)/bin/tau

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/tau
	rm -f $(DESTDIR)$(PREFIX)/bin/taud

.PHONY: all clean install uninstall
