.POSIX:

PREFIX = /usr/local
CONFDIR = $(HOME)/.config/darkfi
DLURL = http://185.165.171.77

CARGO = cargo
DLTOOL = wget -nv --show-progress -O-
#DLTOOL = curl

# Here it's possible to append "cashierd" and "gatewayd".
BINS = drk darkfid

all: $(BINS) uid confdir mint.params spend.params

$(BINS): $(shell find src -type f)
	$(CARGO) build --release --all-features --bin $@
	cp target/release/$@ $@

uid:
	id -u > $@

confdir:
	@echo "$(CONFDIR)" > $@

%.params:
	$(DLTOOL) $(DLURL)/$@ > $@

test:
	$(CARGO) test --release --all-features

fix:
	$(CARGO) fix --release --all-features --allow-dirty

clippy:
	$(CARGO) clippy --release --all-features

install:
	@if ! [ -f uid ]; then \
		echo "Please run 'make' as user first." ; \
		exit 1 ; \
	fi;
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BINS) $(DESTDIR)$(PREFIX)/bin
	mkdir -p "$(shell cat confdir)"
	for i in $(BINS); \
	do \
		cp example/config/$$i.toml "$(shell cat confdir)" ; \
	done;
	cp mint.params spend.params "$(shell cat confdir)"
	chown -R "$(shell cat uid):$(shell cat uid)" "$(shell cat confdir)"

uninstall:
	for i in $(BINS); \
	do \
		rm -f $(DESTDIR)$(PREFIX)/bin/$$i; \
	done;

clean:
	rm -f $(BINS) mint.params spend.params uid confdir

distclean: clean
	rm -rf target

.PHONY: all test fix clippy install uninstall clean distclean
