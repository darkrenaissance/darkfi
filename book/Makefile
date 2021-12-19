.POSIX:

all:
	mdbook build

github: all
	ghp-import -m "Generate mdbook" -b gh-pages book/html

.PHONY: all github
