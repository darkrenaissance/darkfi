The DarkFi book
===============

This directory contains the sources for the book that can be read on
https://darkrenaissance.github.io/darkfi

When adding or removing a section of the book, make sure to update the
[SUMMARY.md](src/SUMMARY.md) file to actually list the contents.

Use a python virtual environment to install its requirements:
```shell
% python -m venv venv
% source venv/bin/activate
```

Then install the requirements:

```shell
% pip install -r requirements.txt
```

Using the Makefile to build the sources requires the Rust `mdbook`
utility which may be installed via:

```shell
cargo install mdbook
```

For the plugin mdbook backends run:

```
cargo install --git "https://github.com/lzanini/mdbook-katex"
cargo install --git "https://github.com/badboy/mdbook-toc"
cargo install --git "https://github.com/badboy/mdbook-mermaid"
cargo install --git "https://github.com/rustforweb/mdbook-plugins" mdbook-tabs
```
