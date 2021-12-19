Notes for developers
====================

## `cargo fmt` pre-commit hook

To ensure every contributor uses the same code style, make sure
you run `cargo fmt` before committing. You can force yourself to do
this by creating a git `pre-commit` hook like the following:

```
#!/bin/bash
diff="$(cargo fmt -- --check)
result=$?

if [[ "$result" -ne 0 ]]; then
    echo "There are some code style issues. Run 'cargo fmt' to fix it."
    exit 1
fi

exit 0
```

Place this script in `.git/hooks/pre-commit` and make sure it's
executable by running `chmod +x .git/hooks/pre-commit`.
