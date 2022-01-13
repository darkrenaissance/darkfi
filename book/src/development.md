Notes for developers
====================

## Making life easy for others

> **Write useful commit messages.**

If your commit is changing a specific module in the code and not
touching other parts of the codebase (as should be the case 99% of the
time), consider writing a useful commit message that also mentions
which module was changed.

For example, a message like:

> `added foo`

is not as clear as

> `crypto/keypair: Added foo method for Bar struct.`

Also keep in mind that commit messages can be longer than a single
line, so use it to your advantage to explain your commit and
intentions.


## cargo fmt pre-commit hook

To ensure every contributor uses the same code style, make sure
you run `cargo +nightly fmt` before committing. You can force yourself
to do this by creating a git `pre-commit` hook like the following:

```shell
#!/bin/sh
if ! cargo +nightly fmt -- --check >/dev/null; then
    echo "There are some code style issues. Run 'cargo fmt' to fix it."
    exit 1
fi

exit 0
```

Place this script in `.git/hooks/pre-commit` and make sure it's
executable by running `chmod +x .git/hooks/pre-commit`.


## Testing crate features

Our library heavily depends on cargo _features_. Currently
there are more than 650 possible combinations of features to
build the library.  To ensure everything can always compile
and works, we can use a helper for `cargo` called
[`cargo hack`](https://github.com/taiki-e/cargo-hack).

The `Makefile` provided in the repository is already set up to use it,
so it's enough to install `cargo hack` and run `make check`.
