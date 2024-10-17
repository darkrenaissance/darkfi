darkfi-serial
=============

Take note that this crate and its inner crates are published on
crates.io so when they're used, they should be imported as a regular
dependency rather than using a local repo path.

When changes are made to the serial library, we should bump its version
and perform a `cargo publish`.

Eventually we can investigate a path of also exporting `darkfi-serial`
through `darkfi-sdk` so that dependency management simplifies a bit.
