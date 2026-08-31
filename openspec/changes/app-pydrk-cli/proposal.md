## Why

The `enable-netdebug` backend (ZeroMQ REQ/REP on 9484, PUB on 9485) is the
supported way to inspect and drive a running `app` GUI, but the only clients
are ad-hoc scripts (`bin/app/script/`) and the `pydrk` library — there is no
CLI. Worse, the node-mutation commands (`AddNode`, `LinkNode`, `RemoveNode`,
...) in `bin/app/src/net.rs` were commented out when the central scene-graph
registry was removed, so the `pydrk` client methods for them are dead: the
server replies `errc=0` with an empty body and the client misparses it.
Inspecting and experimenting with the live scene graph currently requires
writing a custom Python script for every question. A junior dev learning the
wallet UI has no safe, incremental tool to explore nodes, tweak properties,
or draw shapes without recompiling the app.

## What Changes

- Re-enable wire-level node creation in `bin/app/src/net.rs` against the
  current path-addressed tree model:
  - `AddNode` becomes atomic and path-based: payload
    `(parent_path, name, node_type)` → replies the new `node_id`; the node
    is created via the existing Rust factories and linked immediately.
    Supported types for v1: `Layer` and `VectorArt` (the two factories whose
    pimpls only need `Renderer` + `RedrawTrigger`); other types fail with
    `PropertyWrongType`-style rejection (see design for error choice).
  - `RemoveNode` becomes path-based: payload `(node_path)`; unlinks the
    subtree from its parent and triggers a redraw (existing `Drop` impls
    clear GPU draw calls).
  - `ZeroMQAdapter` gains the `Renderer` handle so `Layer::new` /
    `VectorArt::new` pimpls can be wired for wire-created nodes.
- Update `pydrk/api.py` to match: new `add_node(parent_path, name, node_type)`
  and `remove_node(node_path)` methods; remove the dead id-based client
  methods that no longer have server arms (`get_info`, `get_parents`,
  `link_node`, `unlink_node`, `rename_node`, `scan_dangling`,
  `lookup_node_id`, `add_property`, `unregister_slot`, `lookup_slot_id`).
- Fix `pydrk` `get_method()` result parsing: the server sends
  `Option<Vec<CallArg>>` but the client decodes a bare array, which
  misparses any method that declares a result.
- Add a `pydrk` CLI (`python -m pydrk ...` via a new `__main__.py` +
  `cli.py`, argparse-based, only dependency stays pyzmq) with subcommands
  for the whole scene-graph workflow: connectivity check, tree navigation
  (`ls`, `tree`), display (`props`, `get`), property setting (`set`,
  type-driven by server metadata, incl. exprs), node creation/removal
  (`mknode`, `rmnode`), shape data (`set-shape` composing the existing
  `pydrk.vector_shape` builders), and introspection of signals/methods
  (`signals`, `methods`, `call`).
- Add an interactive shell mode: running `python -m pydrk` with no
  subcommand drops into a REPL over the same command set, maintaining a
  current working node path (`cd`, `pwd`) so `ls` lists the current node's
  children and properties and `set foo XXX` writes a property of the
  current node. Tab completion (stdlib `readline`) completes command
  names, node paths (children fetched live from the app), and property
  names.

Non-goals (recorded so they are not silently assumed): no packaging
(pyproject/console script) — the CLI runs from `bin/app` like the existing
scripts; no new prompt/REPL dependency (prompt_toolkit & co.) — stdlib
`readline` only; no rename/link/unlink of pre-existing nodes; no new
event/PUB commands; no changes to release builds (`enable-netdebug` stays
dev-only).

## Capabilities

### New Capabilities
- `pydrk-cli`: Command-line access to a running app's scene graph over the
  netdebug backend, in one-shot and interactive forms — tree navigation and
  display, property get/set (typed values and exprs), node creation/removal
  for Layer and VectorArt, shape data composition, method introspection,
  and a shell mode with `cd` and tab completion.

### Modified Capabilities

(none — `openspec/specs/` is empty; there are no existing main specs to
modify.)

## Impact

- Rust: `bin/app/src/net.rs` (re-enable + redesign `AddNode`/`RemoveNode`,
  carry `Renderer`), `bin/app/src/main.rs` (pass renderer into
  `ZeroMQAdapter::new`), possibly `bin/app/src/error.rs` (only if a new
  error code is needed — design prefers reusing existing ones).
- Python: `bin/app/pydrk/api.py` (new/removed methods, `get_method` fix),
  new `bin/app/pydrk/cli.py` and `bin/app/pydrk/__main__.py`; existing
  modules (`serial.py`, `print_tree.py`, `vector_shape.py`, `exc.py`) are
  reused as-is.
- Wire protocol: payloads of commands 1 (`AddNode`) and 9 (`RemoveNode`)
  change shape. Both sides live in this repo and ship together; the
  netdebug backend is dev-only (feature-gated out of release builds), so
  there are no compatibility constraints.
- Docs: usage examples live in this change's design; `doc/src/arch/wallet.md`
  still references the pre-rename `bin/darkwallet/pydrk` path — fixing that
  is left to a docs change.
- No workspace-level `make` targets are affected; Rust verification is
  `make compile-dev` (and `make compile-apk` for android), Python is run
  from `bin/app`.
