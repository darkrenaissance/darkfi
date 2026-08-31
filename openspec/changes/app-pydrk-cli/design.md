## Context

The netdebug backend (`bin/app/src/net.rs`, feature `enable-netdebug`,
dev builds only) serves ZeroMQ REQ/REP on `:9484` and PUB on `:9485`.
Requests are 2 frames `[cmd:1][payload]`, replies `[errc:1][body]`, with
`darkfi-serial` encoding inside frames. The Python client `pydrk`
(`bin/app/pydrk/`) mirrors the codec (`serial.py`), the command/error
tables (`api.py`), and shape builders (`vector_shape.py`).

The scene graph is now a tree of `SceneNode` addressed by `/`-separated
paths (`ScenePath`), looked up from `sg_root` by walking child names.
Nodes are built by Rust factories (`create_layer`, `create_vector_art`,
... in `src/app/node.rs`) that attach the factory's properties, then
`.setup(|me| Layer::new(me, renderer, redraw)).await` installs the pimpl,
then `parent.link(node)` attaches it. The old central node registry is
gone, which is why the id-based `AddNode`/`LinkNode`/... arms in
`net.rs` are commented out — they reference a `scene_graph` object that
no longer exists. `SceneNode::link()` asserts the child has no parent
yet, and the pimpl types clean up GPU draw calls in `Drop`
(`VectorArt::drop` calls `replace_draw_calls`).

Only a subset of `Command` arms are live today: `Hello`, `GetChildren`,
`GetProperties`, `GetPropertyValue`, `SetPropertyValue` (incl. expr
compile and full `VectorShape` push), `GetSignals`, `RegisterSlot`,
`GetSlots`, `GetMethods`, `GetMethod`, `CallMethod`.

The existing entry points are example scripts (`bin/app/script/`) and the
`pydrk` library — no CLI exists. `pydrk` has no packaging; it is run with
cwd `bin/app`. Its sole dependency is pyzmq. Self-testing convention is an
`if __name__ == "__main__":` block (see `vector_shape.py`).

## Goals / Non-Goals

**Goals:**

- One-shot CLI subcommands and an interactive shell over the same code,
  usable by a junior dev to explore and drive a running dev-mode app.
- Wire-level creation/removal of `Layer` and `VectorArt` nodes with full
  factory properties and live pimpls, so shapes drawn over the wire show
  up and clean up correctly.
- Keep both sides of the protocol inside this repo and in lockstep.

**Non-Goals:**

- No packaging, no new Python dependencies (stdlib `readline` +
  `argparse` + `shlex`; pyzmq stays the only third-party import).
- No wire support for widget types needing app context (`Text`, `Edit`,
  `ChatView`, plugins, ...). Only `Layer` and `VectorArt`.
- No rename/relink of pre-existing nodes; no event subscription commands
  (the PUB-side `EventLoop` in `pydrk/event.py` stays as is).
- No changes to release builds; netdebug stays behind the dev feature.

## Decisions

### D1: Node creation is path-based and atomic

`AddNode` carries `(parent_path: String, name: String,
node_type: SceneNodeType)` and replies the new `node_id: u32`. The server
resolves the parent, builds the node, links it, and registers it — one
round trip, no dangling state.

Alternative considered: resurrect the old two-step
`AddNode(name, type) -> id` + `LinkNode(child_id, parent_id)`. That needs
a server-side `id -> node` registry to keep dangling nodes addressable,
which is exactly the machinery that was deleted with the central
registry. The CLI never wants a dangling node. Atomic attach is simpler
and matches the path-addressed model.

### D2: Only `Layer` and `VectorArt` factories are exposed

Both pimpls take only `(SceneNodeWeak, Renderer, RedrawTrigger)` — the
two handles the adapter can carry. The `AddNode` arm (conceptually):

```rust
Command::AddNode => {
    let parent_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
    let node_name = String::decode(&mut cur).unwrap()?;
    let node_type = SceneNodeType::decode(&mut cur).unwrap();
    debug!(target: "req", "{cmd:?}({parent_path}, {node_name}, {node_type:?})");

    let parent = self.sg_root.lookup_node(parent_path).ok_or(Error::NodeNotFound)?;

    if parent.get_children().iter().any(|c| c.name == node_name) {
        return Err(Error::NodeSiblingNameConflict)
    }

    let node = match node_type {
        SceneNodeType::Layer => {
            create_layer(&node_name)
                .setup(|me| Layer::new(me, self.renderer.clone(), self.redraw.clone()))
                .await
        }
        SceneNodeType::VectorArt => {
            create_vector_art(&node_name)
                .setup(|me| VectorArt::new(me, self.renderer.clone(), self.redraw.clone()))
                .await
        }
        _ => return Err(Error::UnsupportedNodeType),
    };

    self.redraw.make_guard(gfxtag!("ZeroMQAdapter::AddNode"));
    parent.link(node.clone());
    node.id.encode(&mut reply).unwrap();
}
```

Notes: `SceneNode::setup` must run before `link` (it asserts
`strong_count == 1`), same ordering as every schema call site. After
link, the pimpl's `start(ex)` is spawned (`self.ex.spawn(...)`) so
`OnModify` handlers (redraw on property change) are armed exactly like
window-owned nodes — copy the pattern from `src/ui/win/mod.rs`
(`obj.start(ex.clone()).await`). Factories and pimpls are imported from
where the schema uses them (`crate::app::node::{create_layer,
create_vector_art}`, `crate::ui::{Layer, VectorArt}`).

Other node types fail with a new `Error::UnsupportedNodeType` (D4). The
client additionally rejects unknown type strings locally before sending
(friendlier message, no round trip).

### D3: Removal unlinks the subtree, with full graph access

`RemoveNode` carries `(node_path: String)`. Flow: reject `/` (the root
has no parent, so removal is meaningless) with `Error::NodeNotRemovable`;
look up the node; `node.unlink()`; `self.redraw.trigger()`. When the
parent drops its last `Arc`, the pimpl `Drop` impls clear GPU draw calls
and the `OnModify` tasks die with the node.

This is a debugging tool, so removal is deliberately NOT restricted:
built-in nodes are removable exactly like wire-created ones, giving the
tool full access to the scene graph. The safety story is that netdebug
is dev-only and every change is runtime-only — restarting the app
restores the schema-built tree. Removing nodes the render pass depends
on (e.g. `/window`) can leave a blank window or, in the worst case, an
app panic; that is an accepted, documented trade-off for a debug tool,
and the app is simply restarted.

Alternative considered: restrict removal to wire-created nodes (a
`wire_nodes` id set in the adapter). Rejected per requirements — the
point of the tool is to experiment on the real tree, including taking
subtrees away.

### D4: Two new error codes, mirrored on both sides

`Error::UnsupportedNodeType = 50` and `Error::NodeNotRemovable = 51` (the
latter used only to reject removing the scene root) in `src/error.rs`
(the enum ends at 49). pydrk mirrors them: `ErrorCode` entries + `exc.py`
classes + `_make_request` match arms. Existing errors cover everything
else (sibling conflict → `NodeSiblingNameConflict`, missing parent →
`NodeNotFound`).

### D5: The adapter carries the `Renderer`

`ZeroMQAdapter::new(sg_root, renderer, redraw, ex)` — `main.rs` already
has `app.renderer` in scope where the adapter is spawned (currently
`main.rs:196-207`), so this is a one-line call-site change plus the
struct field.

### D6: pydrk client updates

- New: `add_node(parent_path, name, node_type) -> int` and
  `remove_node(node_path)` in `api.py`, encoded per D1/D3.
- Removed: dead client methods whose server arms are gone (`get_info`,
  `get_parents`, `link_node`, `unlink_node`, `rename_node`,
  `scan_dangling`, `lookup_node_id`, `add_property`, `unregister_slot`,
  `lookup_slot_id`) and the `vertex()`/`face()` legacy mesh helpers.
  They misparse the empty errc=0 replies and only encode the removed
  registry world.
- Fixed: `get_method()` result decoding. The server encodes
  `Option<Vec<CallArg>>` (`net.rs` `method.result.encode(...)`);
  `api.py` currently decodes a bare array, which silently truncates for
  `Some(...)` results. Read the option tag first:

```python
args = serial.decode_arr(cur, read_arg)
results = serial.decode_opt(cur, lambda cur: serial.decode_arr(cur, read_arg))
```

### D7: CLI architecture — one module, shared handlers

```
bin/app/pydrk/cli.py      argparse subcommands + handler functions + REPL + completer
bin/app/pydrk/__main__.py import cli; cli.main()
```

- `main()` builds a parser with `--addr`/`--port` and one subparser per
  command. With a subcommand: run the handler once, print errors as
  `error: <name>`, `sys.exit(1)`. With no subcommand: enter the shell.
- Handlers are small functions `cmd_ls(api, args)`, `cmd_set(api, args)`
  ... shared by both modes. The shell re-parses each input line with
  `shlex.split` and dispatches to the same per-command arg parsers, so
  usage/help stays single-sourced in argparse.
- Every handler receives paths already resolved to absolute (see D9), so
  handlers never see cwd.
- Errors: pydrk exceptions are caught at the dispatch boundary; one-shot
  mode exits non-zero, shell mode prints and continues.

### D8: Typed property get/set/show driven by server metadata

`set`, `get` and `show` share one positional grammar with optional parts:
`set [path] PROP [idx] VAL`, `get [path] PROP [idx]`, `show [path] PROP`.
Positionals are parsed right-to-left so no flags are needed for the
common cases (`VAL` is always last; an integer right before it is the
index; what is left at the front is the path, joined with `/` when it
spans several tokens). Flags (`--expr`) are stripped first:

```python
def parse_set_args(tokens, default_path):
    value = tokens.pop()
    idx = 0
    if tokens and tokens[-1].isdigit():
        idx = int(tokens.pop())
    if not tokens:
        raise UsageError("missing property name")
    prop = tokens.pop()
    path = resolve_path(default_path, "/".join(tokens)) if tokens else default_path
    return (path, prop, idx, value)
```

`parse_get_args`/`parse_show_args` are the same idea without `VAL`
(property names are never integers, so the right-to-left split is
unambiguous; the one-shot mode passes `default_path="/"`). `show` prints
one property's metadata block (same fields as `props`, plus depends)
followed by its per-index values — the "everything about this property"
view. `set` fetches `api.get_properties(path)` once, finds the property,
and encodes by its declared type:

| declared type | encoding | value parsing |
|---|---|---|
| bool | `set_property_bool` | `true`/`false` |
| uint32 / scene_node_id | `set_property_u32` / `set_property_node_id` | `int(token, 0)` |
| float32 | `set_property_f32` | `float(token)` |
| str | `set_property_str` | token as-is (quote in shell for spaces) |
| enum | `set_property_enum` | must be in `enum_items`, else local error |
| null literal `null` | `set_property_null` | n/a |

The `--expr` flag switches to `set_property_expr` and sends the value as
expr source (the server compiles with the const-free compiler; `w`/`h`
are the available globals). Enum membership and numeric parsing are
validated client-side so mistakes produce a local usage error instead of
a wire round trip. This "ask the server for the type" approach means
juniors never have to know the type — `set alpha 0.5` just works.

### D9: Interactive shell

State: `Shell` class holding the `Api`, `cwd: list[str]` (tokens, `[]` =
root), and the completion cache. Prompt: `pydrk:/window/content> `
(rendered from cwd). Builtins: `cd` (no arg → `/`; `..` pops; otherwise
resolve and verify with `api.get_children(parent)` before committing),
`pwd`, `exit`/`quit` (and EOF). Everything else dispatches through the
same handlers as one-shot mode.

Path resolution (pure function, unit-tested):

```python
def resolve_path(cwd, arg):
    if arg.startswith("/"):
        tokens = arg.split("/")
    else:
        tokens = cwd + arg.split("/")
    out = []
    for token in tokens:
        if token in ("", "."):
            continue
        if token == "..":
            if out:
                out.pop()
            continue
        out.append(token)
    return "/" + "/".join(out)
```

Absolute arguments (leading `/`) pass through unchanged; all others are
taken relative to cwd. The `cd` of a resolvable-but-childless path is
still valid (leaves can be cwd for `set`/`get`); `cd` into a
non-resolvable path fails with `node_not_found` and leaves cwd alone —
verified by looking the path up (`api.get_children` of the parent, or a
cheap `api.get_properties(path)`).

Line tokenization is `shlex.split` so values containing spaces can be
quoted: `set nick "hello world"`.

### D10: Tab completion via stdlib readline

A `Completer` class registered with `readline`:

- First token: complete from the command-name table.
- `get`/`set`/`show` first argument: complete from the union of the cwd
  node's property names (`api.get_properties`, cached) and its child node
  paths — both are valid leading tokens under the optional-path grammar
  of design D8.
- Any other argument: complete child node names. Split the token into
  dir part + prefix (last `/`), resolve the dir part against cwd, fetch
  `api.get_children` (cached), return `name + "/"` matches. Matches come
  from the live app, so freshly `mknode`-ed nodes complete immediately.
- The cache is a dict `path -> [child names / prop names]` cleared at
  every prompt redraw (i.e. after each executed command), so mutations
  are picked up without staleness bugs. One REQ per uncached directory
  per line — the REQ/REP socket is lockstep anyway.
- `import readline` is wrapped in try/except; without it the shell runs
  without completion (e.g. exotic platforms). Dev target is Linux.

No `rlcompleter`/`prompt_toolkit`: zero new dependencies, and
`Completer` needs app state (cwd, live children) that the generic
completer doesn't have.

### D11: `set-shape` composes `vector_shape.VectorShape` from flags

Each primitive flag is `action="append"` and carries its colors inline as
trailing `R G B A` float args (no hidden color state):

```
--box X1 Y1 X2 Y2 R G B A
--gbox X1 Y1 X2 Y2 R G B A R G B A        (top color, bottom color)
--vgradient X1 Y1 X2 Y2 R G B A R G B A STRIPS GAMMA
--outline X1 Y1 X2 Y2 BORDERPX R G B A
--line X1 Y1 X2 Y2 THICKNESS R G B A
--glow CX CY W H SEGMENTS R G B A
```

Coordinates are passed through to `vector_shape` as-is: plain numbers
are normalized to float literals, anything else (e.g. `w/2`, `h - 10`)
is expr source — exactly what `VectorShape._coord` already does. Flags
apply in command-line order via `shape.join(...)`. Client-side guard:
`len(shape.verts) < 65536` before sending (indices are u16 on the wire).
Then `shape.set(api, path, prop_name)` pushes it. Example:

```
python -m pydrk set-shape /window/content/dbg/art1 --box 0 0 w 10 1 0 0 1
```

### D12: Testing strategy

- Rust: `make compile-dev` after every Rust task (per `bin/app/AGENTS.md`).
- Python pure logic (path resolution, typed-value parsing, color/coord
  parsing, shape flag composition) is exercised by
  `python -m pydrk.cli --selftest` — an `if __name__ == "__main__"`-style
  assert block, same convention as `python -m pydrk.vector_shape`.
- Live behavior: run `make dev` in one terminal, CLI in another; each
  task lists the exact commands and expected output. The drawing tasks
  are verified by looking at the window.
- Commits after every task, message prefix `app:` or `app/netdebug:`.

## Risks / Trade-offs

- [All mknode/rmnode changes are runtime-only and lost on restart] →
  Documented behavior and the recovery path for destructive removals;
  the schema-built tree is rebuilt on every launch.
- [Removing a node the render pass depends on can blank the window or
  panic the app] → Accepted for a dev-only debug tool with full graph
  access; restart recovers. Documented in `rmnode --help`.
- [Dropping a subtree relies on the parent holding the last strong ref]
  → Verified live in the removal task (window shows removal + no stale
  geometry). `OnModify` holds only weak refs; if anything is found
  holding a strong ref, removal degrades to "hidden but alive", which is
  still safe — escalate before shipping if observed.
- [Shape eval errors are silent to the client] → The server logs a warn
  and draws nothing; the CLI cannot see it. Documented in usage; the
  invalid-expr rejection path (`sexpr_global_not_found`) is still
  surfaced because it fails at compile time, before eval.
- [readline differs on macOS/libedit] → Completion is best-effort and
  guarded; Linux dev machines are the target.
- [REQ/REP lockstep] → The shell is strictly one request at a time;
  completion caches prevent request storms while tabbing.
- [Protocol change for AddNode/RemoveNode payloads] → Both peers ship in
  the same repo and the backend is dev-only; no migration needed. Old
  clients against a new app fail fast on decode, not silently.

## Migration Plan

None. The netdebug backend is feature-gated out of release builds; dev
workflows rebuild both peers from the same tree.

## Open Questions

- Should a `/debug` parent layer be created at app boot to host wire
  nodes by default? Deferred: users can `mknode` anywhere under
  `/window/content` today; adding a fixed parent is cosmetic and can be
  a follow-up.
- Should `pydrk/event.py`'s `EventLoop` keyboard path be repointed to a
  live node path? Deferred: out of scope for this change (PUB-side
  tooling unchanged).
