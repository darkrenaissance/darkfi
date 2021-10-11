.POSIX:

CARGO = cargo
PREFIX = /usr/local
CONFDIR = $(HOME)/.config/darkfi

all:
	mkdir -p $(CONFDIR)
	@echo "$(CONFDIR)" > .confdir
	$(CARGO) build --release --all-features

test:
	$(CARGO) test --release --all-features

fix:
	$(CARGO) fix --release --all-features --allow-dirty

clippy:
	$(CARGO) clippy --release --all-features

install:
	@if ! [ -f target/release/drk ]; then \
		echo "Please run 'make' as user first." ; \
		exit 1 ; \
	fi;
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f target/release/cashierd $(DESTDIR)$(PREFIX)/bin
	cp -f target/release/darkfid $(DESTDIR)$(PREFIX)/bin
	cp -f target/release/drk $(DESTDIR)$(PREFIX)/bin
	cp -f target/release/gatewayd $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/cashierd
	chmod 755 $(DESTDIR)$(PREFIX)/bin/darkfid
	chmod 755 $(DESTDIR)$(PREFIX)/bin/drk
	chmod 755 $(DESTDIR)$(PREFIX)/bin/gatewayd
	cp example/config/*.toml "$(shell cat .confdir)"

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/cashierd
	rm -f $(DESTDIR)$(PREFIX)/bin/darkfid
	rm -f $(DESTDIR)$(PREFIX)/bin/drk
	rm -f $(DESTDIR)$(PREFIX)/bin/gatewayd

.PHONY: all test fix clippy install uninstall
