.POSIX:

# Install prefix
PREFIX = /usr/local

# Cargo binary
CARGO = cargo

# Binaries to be built
BINS = drk darkfid gatewayd

# Common dependencies which should force the binaries to be rebuilt
BINDEPS = \
	Cargo.toml \
	$(shell find src -type f) \
	$(shell find sql -type f) \
	$(shell find contrib/token -type f)

all: $(BINS)

$(BINS): $(BINDEPS)
	$(MAKE) -C bin/$@
	cp -f bin/$@/$@ $@

check:
	$(CARGO) hack check --release --feature-powerset --no-dev-deps

fix:
	$(CARGO) clippy --release --all-features --fix --allow-dirty

clippy:
	$(CARGO) clippy --release --all-features

test:
	$(CARGO) test --release --all-features

clean:
	rm -f $(BINS)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	mkdir -p $(DESTDIR)$(PREFIX)/share/darkfi
	mkdir -p $(DESTDIR)$(PREFIX)/share/doc/darkfi
	cp -f $(BINS) $(DESTDIR)$(PREFIX)/bin
	for i in $(BINS); \
	do \
		cp -f example/config/$$i.toml $(DESTDIR)$(PREFIX)/share/doc/darkfi; \
	done;

uninstall:
	for i in $(BINS); \
	do \
		rm -f $(DESTDIR)$(PREFIX)/bin/$$i; \
	done;
	rm -rf $(DESTDIR)$(PREFIX)/share/doc/darkfi
	rm -rf $(DESTDIR)$(PREFIX)/share/darkfi

.PHONY: all check fix clippy test clean install uninstall
