The DarkFi book
===============

This directory contains the sources for the book that can be read on
https://darkrenaissance.github.io/darkfi

When adding or removing a section of the book, make sure to update the
[SUMMARY.md](src/SUMMARY.md) file to actually list the contents.

 
```
cargo install mdbook
make
```

to run and watch for changes :
```
mdbook serve --open
```

For the mdbook-katex backend run:

```
cargo install --git "https://github.com/lzanini/mdbook-katex"
cargo install --git "https://github.com/badboy/mdbook-toc"
cargo install --git "https://github.com/badboy/mdbook-mermaid"
```
