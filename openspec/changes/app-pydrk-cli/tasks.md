## 1. CLI scaffold and inspection commands (Python only)

Work happens in `bin/app/`. For live tests run `make dev` in a second
terminal and keep it running; every `python -m pydrk ...` line below is
run from `bin/app`.

- [x] Create `pydrk/cli.py` (argparse `main()`, global `--addr`/`--port`
  defaulting to `127.0.0.1:9484`, subcommand dispatch, top-level
  try/except printing `error: <name>` and exiting 1) and
  `pydrk/__main__.py` (`from pydrk import cli; cli.main()`). Implement
  only `ping` using `Api.hello()`. Verify: `python -m pydrk ping` prints
  `hello` against the running app; `python -m pydrk ping --port 9999`
  prints an error naming `127.0.0.1:9999` and exits non-zero. Commit as
  `app: add pydrk CLI skeleton with ping`.
- [x] Implement `ls [path]`: child rows as `name <id> type` (type via
  `SceneNodeType` names) followed by property rows `name: type = value`
  (value from `get_property_value`, exprs shown as their source,
  `<shape>` placeholder for shapes). Verify: `python -m pydrk ls /`
  lists `setting` and `window` plus the root's properties;
  `python -m pydrk ls /nope` prints `node_not_found`. Commit as
  `app: pydrk CLI ls command`.
- [x] Implement `tree [path] [--depth N]` by wiring
  `pydrk.print_tree.print_tree` into the CLI. Verify: `python -m pydrk
  tree / --depth 2` prints two levels with properties, signals and
  methods. Commit as `app: pydrk CLI tree command`.
- [x] Implement `props <path>`: one block per property showing name,
  type, subtype, array_len (mark unbounded when 0), null/expr allowance,
  min/max range when present, enum items when present, ui_name and desc.
  Verify: `python -m pydrk props /window/content` shows `alpha` with its
  `[0.0, 1.0]` range. Commit as `app: pydrk CLI props command`.
- [x] Implement `get [path] PROP [idx]` with the shared positional
  grammar from design D8 (right-to-left parse via `parse_get_args`, path
  optional defaulting to `/` in one-shot mode, trailing integer = index):
  one line per index annotated `value`/`expr`/`null`/`unset`, only the
  given index when `idx` is present. Add `parse_get_args` cases to the
  selftest. Verify against the running app: `python -m pydrk get
  /window/content alpha` prints `0: value 1.0`; `python -m pydrk get
  /window/content rect 2` prints only index 2. Commit as
  `app: pydrk CLI get command`.
- [x] Implement `show [path] PROP`: the full single-property view
  from design D8 — metadata block (name, type, subtype, array_len,
  null/expr allowance, min/max range, enum items, ui_name, desc,
  depends) followed by the per-index values with statuses. Verify: `python
  -m pydrk show /window/content alpha` prints the metadata including the
  `[0.0, 1.0]` range and then `0: value 1.0`; `python -m pydrk show
  /window/content no_such_prop` prints `property_not_found`. Commit as
  `app: pydrk CLI show command`.
- [x] Fix `Api.get_method()` in `pydrk/api.py`: decode the results as
  `Option<Vec<CallArg>>` (read the u8 tag, then the array only when
  some) per design D6. Implement `methods <path>` (name + arg/result
  signature per method) and `signals <path>` (signal names). Verify:
  `python -m pydrk methods /plugin/drk` lists `get_default_address`
  with its `str` result signature, and `python -m pydrk tree /plugin/drk`
  no longer truncates method results. Commit as
  `app: fix pydrk get_method result decoding, add methods/signals commands`.
- [x] Add `--selftest` handling in `cli.py`: a `run_selftests()`
  function with assert-based checks of the pure helpers introduced so
  far (path/type/value formatting, `parse_get_args`), so `python -m
  pydrk.cli --selftest` prints `cli self-test OK` without a running app.
  Commit as `app: pydrk CLI selftest harness`.

## 2. Typed property setting (Python only)

- [x] Implement the typed value table from design D8: a pure
  `encode_set_value(api, path, prop_meta, token, index)` helper that
  looks up the property via `get_properties` and dispatches to the right
  `Api.set_property_*` call (bool/uint32/float32/str/enum/scene_node_id,
  `null` literal → `set_property_null`, enum membership validated
  locally). Wire it into `set [path] PROP [idx] VAL` with the
  right-to-left `parse_set_args` (path optional, trailing-integer index,
  leading path tokens joined with `/`, usage error when the property
  name is missing). Verify live: `python -m pydrk set
  /window/content/chat is_visible false` hides the chat UI, then `true`
  restores it; `python -m pydrk get /window/content/chat is_visible`
  round-trips both values. Add `parse_set_args` cases to `--selftest`.
  Commit as `app: pydrk CLI typed set command`.
- [x] Add `--expr` to `set` (sends via `set_property_expr`). Verify
  live: `python -m pydrk set /window/content rect 2 "w/2" --expr`
  exits 0 and `python -m pydrk get /window/content rect 2` shows
  `2: expr "w/2"`. Commit as `app: pydrk CLI set --expr`.
- [ ] 2.3 Verify the failure paths end-to-end: `python -m pydrk set
  /window/content alpha 5.0` prints `property_out_of_range`;
  `python -m pydrk set /window no_such_prop 1` prints
  `property_not_found`; `set --expr "q/3"` on a rect index prints
  `sexpr_global_not_found`; `python -m pydrk set` alone prints a usage
  error; all exit non-zero and none change app state.
  Fix anything that prints a raw traceback instead of `error: <name>`.
  Commit as `app: pydrk CLI set error reporting`.

## 3. netdebug backend: node creation and removal (Rust)

- [x] Add `Error::UnsupportedNodeType = 50` and
  `Error::NodeNotRemovable = 51` (used to reject removing the scene
  root) to `bin/app/src/error.rs` following the existing variant style.
  Verify: `make compile-dev` succeeds. Commit as
  `app: add netdebug error codes for node create/remove`.
- [x] Mirror the two codes in pydrk: `ErrorCode` constants, `exc.py`
  exception classes, `_make_request` match arms raising them. Verify:
  `python -m pydrk.cli --selftest` and `python -m pydrk ping` still
  work. Commit as `app: pydrk error codes for node create/remove`.
- [x] Thread the renderer into the adapter per design D5: add the
  `renderer: Renderer` field to `ZeroMQAdapter`, change
  `ZeroMQAdapter::new` to take it, update the call site in `main.rs`
  (it already has `app.renderer` in scope). Verify: `make compile-dev`
  succeeds and `python -m pydrk ping` still works. Commit as
  `app/netdebug: pass renderer into ZeroMQAdapter`.
- [x] Implement the `AddNode` arm per design D2: decode
  `(parent_path, name, node_type)`; look up the parent; reject duplicate
  sibling names with `NodeSiblingNameConflict`; match `Layer` and
  `VectorArt` through `create_layer`/`create_vector_art` +
  `.setup(...)` + spawn pimpl `start(ex)` after `link`; reject other
  types with `UnsupportedNodeType`; reply the id. Verify: `make
  compile-dev` succeeds. Commit as
  `app/netdebug: path-based AddNode for layer and vector_art`.
- [x] Implement the `RemoveNode` arm per design D3: decode
  `(node_path)`; reject `/` with `NodeNotRemovable`; look up the node;
  `unlink()`; `redraw.trigger()`. No restrictions on which nodes are
  removable — full scene-graph access is intentional for this debugging
  tool. Verify: `make compile-dev` succeeds. Commit as
  `app/netdebug: path-based RemoveNode`.

## 4. Node lifecycle commands (Python)

- [x] Add `Api.add_node(parent_path, name, node_type)` to `api.py`
  and the `mknode <parent_path> <name> <type>` subcommand accepting only
  `layer`/`vector_art` (anything else fails locally with
  `unsupported node type`), printing `id=... path=...`. Verify live
  against the rebuilt app: `python -m pydrk mknode /window/content dbg
  layer` prints the id; `python -m pydrk ls /window/content` lists
  `dbg`; `python -m pydrk mknode /window/content/dbg art1 vector_art`
  works and `python -m pydrk props /window/content/dbg/art1` lists the
  factory properties including `shape`; `python -m pydrk mknode
  /window/content dbg layer` again prints `node_sibling_name_conflict`.
  Commit as `app: pydrk CLI mknode command`.
- [x] Add `Api.remove_node(node_path)` and the `rmnode <path>`
  subcommand (full graph access, documented in `--help` as runtime-only).
  Verify live: create `dbg` as above then `python -m pydrk rmnode
  /window/content/dbg` — `ls /window/content` no longer lists it and the
  window redraws; `python -m pydrk rmnode /window/content/chat` removes
  the built-in chat layer (restart the app afterwards to restore it);
  `python -m pydrk rmnode /` prints `node_not_removable` and `python -m
  pydrk ls /` still lists everything. Commit as
  `app: pydrk CLI rmnode command`.
- [x] Delete the dead client methods from `api.py` listed in design
  D6 plus the legacy `vertex()`/`face()` helpers. Verify: `grep -rn
  "link_node\|scan_dangling\|add_property" bin/app/pydrk bin/app/script`
  is empty, `python -m pydrk.cli --selftest` passes, and the commands
  from tasks 1-2 still work live. Commit as
  `app: drop dead pydrk client methods`.

## 5. Shape data (Python)

- [x] Implement `set-shape <path> [--prop NAME] [--index N]` with the
  `--box` flag from design D11 (argparse `nargs=8`, `action="append"`),
  building on `pydrk.vector_shape.VectorShape`, with the
  `< 65536` vertex guard. Verify live: `mknode /window/content dbg
  layer`, `mknode /window/content/dbg art1 vector_art`, set the art
  node's `rect` (e.g. `--expr "w"` at index 2 and `--expr "h"` at
  index 3), `set is_visible true`, then `python -m pydrk set-shape
  /window/content/dbg/art1 --box 0 0 w 10 1 0 0 1` — a red bar renders
  along the top of the window. Commit as
  `app: pydrk CLI set-shape with box primitive`.
- [ ] 5.2 Add the remaining primitives from design D11: `--gbox`,
  `--vgradient`, `--outline`, `--line`, `--glow` (colors inline as
  trailing R G B A float args; coordinates may be expr strings). Verify
  live by composing one shape using at least `--vgradient`, `--outline`
  and `--glow` in a single command and seeing all three render; verify
  `set-shape ... --box 0 0 q/3 10 1 0 0 1` prints
  `sexpr_global_not_found`; verify a >65535-vertex construction is
  rejected client-side. Extend `--selftest` with flag-parsing checks.
  Commit as `app: pydrk CLI set-shape gradient/outline/line/glow`.

## 6. Method calls (Python)

- [x] Implement `call <path> <method> [ARGS...]`: fetch the
  signature with `Api.get_method`, encode each positional arg per its
  declared type (`uint32`/`uint64`/`float32`/`bool`/`str`; `hash` as
  64-char hex → 32 bytes), reject wrong arg counts or unparseable
  tokens locally before sending, print decoded results for `str`/`hash`
  result types and a short hex dump otherwise, `void` when none. Verify
  live: find the chatview node with `python -m pydrk methods
  /window/content/chat` (or `tree`) and run `call <chatview-path>
  copy_select` → prints `void`; `python -m pydrk call /plugin/drk
  get_default_address` prints an address string. Commit as
  `app: pydrk CLI call command`.

## 7. Interactive shell

- [x] Implement `resolve_path(cwd_tokens, arg)` exactly per design
  D9 plus its `--selftest` cases (absolute paths, `..`, `.`, empty,
  relative tokens, leading/trailing slashes). Verify: `python -m
  pydrk.cli --selftest` passes with no app running. Commit as
  `app: pydrk CLI path resolution helper`.
- [x] Implement the shell per design D9: entered when `python -m
  pydrk` runs with no subcommand; prompt `pydrk:/window/content> `;
  `shlex.split` line tokenization; dispatch each line to the same
  per-command handlers as one-shot mode, with the optional-path grammar
  of `set`/`get`/`show` defaulting to cwd (so `set is_visible false`,
  `set rect 2 "w/2" --expr` and `show alpha` all target the cwd node);
  builtins `cd` (no arg → `/`, `..` pops, target existence
  verified, cwd unchanged on failure), `pwd`, `exit`/`quit` + EOF;
  failed commands print `error: <name>` and return to the prompt.
  Verify live session: `cd /window`, `cd content`, `ls`, `show
  is_visible`, `set is_visible false` (chat hides), `get is_visible`
  prints `false`, `set is_visible true`, `cd ..`, `pwd`, `get
  no_such_prop` prints `property_not_found` and the shell survives,
  `exit`. Commit as `app: pydrk interactive shell mode`.
- [x] Implement the readline completer per design D10: command-name
  completion for the first token, live child-path completion (dir part +
  prefix, `api.get_children`, per-prompt cache cleared after each
  executed command), completion of the first positional argument of
  `get`/`set`/`show` from the union of the cwd's property names and
  child paths, guarded `import readline`. Verify live: `cd /win<TAB>`
  completes to `/window/`; `set is_v<TAB>` completes to `is_visible `;
  `show alp<TAB>` completes to `alpha ` on `/window/content`; `mknode
  /window/content dbg layer` then `cd /window/content/db<TAB>`
  completes to `dbg/`; ambiguous prefixes list all matches. Commit as
  `app: pydrk shell tab completion`.

## 8. Final verification

- [x] Run the full junior walkthrough end-to-end against a fresh
  `make dev` instance: `ping`; `ls /`; `tree / --depth 2`; enter the
  shell; `cd /window/content`; `mknode dbg layer` style flow for layer +
  vector_art (via subcommand or shell); set `rect` and `is_visible`;
  `set-shape` a box; `ls` and `get` to confirm state; `rmnode` the debug
  layer and confirm the window redraws clean with no leftover geometry.
  Fix anything broken found during the walkthrough and amend the
  selftest. Commit as `app: pydrk CLI end-to-end walkthrough fixes`.
- [x] Final gates: `make compile-dev` succeeds with no warnings
  introduced; `python -m pydrk.cli --selftest` and `python -m
  pydrk.vector_shape` pass; `python -m pydrk ping` works; `git status`
  shows a clean tree after the last commit. Confirm the spec scenarios
  in `openspec/changes/app-pydrk-cli/specs/pydrk-cli/spec.md` have each
  been exercised at least once during tasks 1-7.
