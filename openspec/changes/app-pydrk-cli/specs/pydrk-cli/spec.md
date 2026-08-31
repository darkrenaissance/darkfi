## Purpose

Command-line access to a running `app` GUI's live scene graph over the
netdebug ZeroMQ backend: navigate and display nodes and properties, set
typed values and exprs, create and remove `Layer`/`VectorArt` nodes, and
push shape data — all without recompiling the app.

## ADDED Requirements

### Requirement: CLI entrypoint and connection

The system SHALL provide a `python -m pydrk` command (runnable from
`bin/app`, argparse-based) whose commands talk to a running app's netdebug
REQ/REP endpoint. The endpoint SHALL default to `127.0.0.1:9484` and be
overridable with `--addr`/`--port` on every subcommand. When no app is
listening, commands SHALL print an error naming the endpoint they tried and
exit non-zero.

#### Scenario: connectivity check

- **WHEN** `python -m pydrk ping` runs against a running dev-mode app
- **THEN** the command prints `hello` and exits 0

#### Scenario: app not running

- **WHEN** a command runs with `--port 9999` and nothing is listening there
- **THEN** the command prints an error containing `127.0.0.1:9999` and exits non-zero

### Requirement: Tree navigation and listing

The `ls [path]` subcommand SHALL list the contents of a scene node: first
its child nodes as one row per child showing the child name, numeric node
id, and lowercase type name (e.g. `content 1234567890 layer`), then its
properties as one row each showing name, type, and current value summary
(exprs as source, shapes as a placeholder). Paths that do not resolve SHALL
produce a readable `node_not_found` error and non-zero exit.

#### Scenario: list the scene root

- **WHEN** `python -m pydrk ls /`
- **THEN** the built-in top-level nodes are listed (at least `setting` and `window`) followed by the root's properties

#### Scenario: unknown path

- **WHEN** `python -m pydrk ls /nope`
- **THEN** the command reports `node_not_found` for `/nope` and exits non-zero

### Requirement: Recursive tree display

The `tree` subcommand SHALL recursively print a node's descendants with a
`--depth N` limit, showing for every node: its name, id, type, properties
with current values, signals with registered slots, and methods with full
signatures. Property values SHALL be rendered distinctly per status: plain
values as literals, exprs as their decompiled source (e.g. `w/2`), null as
`null`, unset-with-default as the default, and vector shapes as a
placeholder (shapes are write-only over the wire). Methods that declare a
result SHALL show the result argument types (not be silently truncated).

#### Scenario: shallow dump

- **WHEN** `python -m pydrk tree / --depth 2`
- **THEN** two levels of the tree print, each property line showing name,
  type and value, and the command exits 0

#### Scenario: method with result renders its signature

- **WHEN** `python -m pydrk tree /plugin/drk`
- **THEN** the `get_default_address` method line includes its result
  signature (a `str` result), demonstrating result decoding

### Requirement: Property metadata and value display

The `props [path]` subcommand SHALL list a node's properties with their
metadata: name, type, subtype, array length (marking unbounded), null/expr
allowance, ranges when bounded, enum items when present, and UI text. The
`show [path] PROP` subcommand SHALL print all info for one property: its
full metadata (as listed for `props`, plus its depends list) followed by
the current per-index values with their statuses. The `get [path] PROP
[idx]` subcommand SHALL print the property's per-index values (only index
`idx` when given), each on its own line annotated with its status
(`value`, `expr`, `null`, or `unset`). For all three, a leading path
argument is optional and defaults to the shell cwd (or `/` in one-shot
mode); in `get`, when the final argument is an integer it is taken as the
index.

#### Scenario: metadata shows bounds and enums

- **WHEN** `python -m pydrk props /window/content`
- **THEN** the `alpha` property (or another bounded one) shows its `[0.0, 1.0]` range

#### Scenario: show a single property

- **WHEN** `python -m pydrk show /window/content alpha`
- **THEN** the output contains the property's metadata (type `float32`,
  the `[0.0, 1.0]` range, its UI text) followed by `0: value 1.0`

#### Scenario: expr value is distinguishable

- **WHEN** a property index holds an expr and `python -m pydrk get` is run for it
- **THEN** the output line is annotated `expr` and shows the expr source string

### Requirement: Typed property setting

The `set` subcommand SHALL set values using the property's server-declared
type for encoding (bool, uint32, float32, str, enum, scene_node_id as
decimal). Its grammar is `set [path] PROP [idx] VAL`: the last argument
is always the value; when the argument before the value is an integer it
is taken as the array index (default 0); any remaining leading arguments
(joined with `/`) are the node path, optional and defaulting to the shell
cwd (or `/` in one-shot mode). The `--expr` flag sends the value as expr
source to be compiled server-side. After a successful set the app SHALL
redraw. Server rejections (wrong type, out-of-range, invalid enum item,
invalid expr syntax, unknown expr global) SHALL be printed readably with
the error name and exit non-zero, and a usage error SHALL be reported
when the arguments cannot be parsed into the grammar (e.g. a property
name is missing).

#### Scenario: set a boolean with an explicit path

- **WHEN** `python -m pydrk set /window/content/chat is_visible false`
- **THEN** the command exits 0, a subsequent `get` shows `false`, and the app window updates

#### Scenario: index and path in one command

- **WHEN** `python -m pydrk set /window/content rect 2 "w/2" --expr`
- **THEN** the command exits 0 and `get /window/content rect 2` shows `expr "w/2"`

#### Scenario: out-of-range rejection

- **WHEN** setting a bounded float32 property to `5.0` when its range is `[0.0, 1.0]`
- **THEN** the command prints `property_out_of_range` and exits non-zero

#### Scenario: expr set round-trip

- **WHEN** `python -m pydrk set <path> rect 2 "w/2" --expr`
- **THEN** the command exits 0 and `get` for that index shows `expr "w/2"`

### Requirement: Node creation

The `mknode` subcommand SHALL create and attach a node in one step:
`mknode <parent_path> <name> <type>` where `<type>` is `layer` or
`vector_art`. On success it SHALL print the new node's id and full path,
and the node SHALL immediately appear in `ls <parent_path>` with all its
factory properties (queryable via `props`). Creating a node whose parent
path does not resolve SHALL fail with `node_not_found`; a name colliding
with an existing sibling SHALL fail with a name-conflict error; any other
type string SHALL fail with a readable `unsupported node type` message
without touching the tree.

#### Scenario: create a debug layer with vector art

- **WHEN** `python -m pydrk mknode /window/content debug_layer layer` then
  `python -m pydrk mknode /window/content/debug_layer art1 vector_art`
- **THEN** both commands print ids and paths, and
  `props /window/content/debug_layer/art1` lists the factory properties
  including `shape`

#### Scenario: unsupported type

- **WHEN** `python -m pydrk mknode /window/content debug_layer chatview`
- **THEN** the command reports the type as unsupported and exits non-zero

### Requirement: Node removal

The `rmnode <path>` subcommand SHALL remove any node subtree from its
parent (the node and its descendants disappear from listings) and trigger
an app redraw; GPU resources owned by removed nodes SHALL be released by
the app. This is a debugging tool with full scene-graph access: built-in
nodes are removable the same way as wire-created ones. Removing the scene
root `/` SHALL fail with a readable error. All removals are runtime-only
and undone by restarting the app.

#### Scenario: remove a wire-created layer

- **WHEN** a layer was created with `mknode` and `python -m pydrk rmnode /window/content/debug_layer` runs
- **THEN** `ls /window/content` no longer lists `debug_layer` and the app redraws

#### Scenario: remove a built-in node

- **WHEN** `python -m pydrk rmnode <path-to-a-built-in-layer>` runs against a running dev app
- **THEN** the subtree disappears from listings and rendering, and restarting the app restores it

#### Scenario: the scene root is not removable

- **WHEN** `python -m pydrk rmnode /`
- **THEN** the command fails with `node_not_removable` and the tree is unchanged

### Requirement: Shape data creation

The `set-shape <path> [--prop NAME] [--index N]` subcommand SHALL build a
vector shape from repeatable primitive flags and push it as the property's
value: `--box X1 Y1 X2 Y2`, `--gbox X1 Y1 X2 Y2` (top and bottom colors),
`--vgradient X1 Y1 X2 Y2 TOPCOLOR BOTCOLOR STRIPS GAMMA`, `--outline X1 Y1
X2 Y2 BORDERPX`, `--line X1 Y1 X2 Y2 THICKNESS`, and `--glow CX CY W H
SEGMENTS COLOR`, each taking colors as `R G B A` float groups. Coordinates
SHALL accept both plain numbers and expr source strings (e.g. `w/2`), and
primitives SHALL join into a single shape in flag order. Shape indices are
16-bit; vertex counts beyond that SHALL be rejected client-side with a
readable message. After a successful set the shape SHALL be visible in the
app window at the next frame (given a non-empty `rect` and `is_visible`).

#### Scenario: draw a red bar over the wire

- **WHEN** a `vector_art` node exists with `rect` set, and
  `python -m pydrk set-shape /window/content/debug_layer/art1 --box 0 0 w 10 --color 1 0 0 1`
  runs (with `w` passed as an expr coordinate)
- **THEN** the command exits 0 and a red bar renders along the top of the node's rect in the app

#### Scenario: invalid expr in shape coordinates

- **WHEN** a coordinate references an unknown global (e.g. `q/3`)
- **THEN** the server rejects the shape and the CLI prints the error name (e.g. `sexpr_global_not_found`) and exits non-zero

### Requirement: Method and signal introspection

The `methods <path>` subcommand SHALL list each method with its argument
signatures and, when declared, result signatures; `signals <path>` SHALL
list signal names. The `call <path> <method> [ARGS...]` subcommand SHALL
encode positional ARGS according to the method's declared argument types
(uint32/uint64/float32/bool/str; hash as 64-char hex), print the decoded
result when the method returns one, and `void` when it does not.

#### Scenario: call a no-result method

- **WHEN** `python -m pydrk call /window/content/chat/view copy_select`
- **THEN** the command prints `void` and exits 0

#### Scenario: argument type mismatch

- **WHEN** calling a method whose first declared argument is `str` with a non-string token where coercion is impossible, or supplying the wrong number of arguments
- **THEN** the CLI rejects it locally with a readable usage error before sending anything

### Requirement: Server error reporting

Every subcommand SHALL map netdebug error frames to the human-readable
error name from the netdebug error table (e.g. `property_not_found`)
together with command context (path, property, method as applicable), and
exit non-zero. Unknown error codes SHALL be printed with their numeric
value.

#### Scenario: unknown property

- **WHEN** `python -m pydrk get /window no_such_prop`
- **THEN** the output contains `property_not_found` and the exit code is non-zero

### Requirement: Interactive shell mode

Running `python -m pydrk` with no subcommand SHALL start an interactive
shell connected to the same endpoint, maintaining a current working node
path (cwd, initially `/`) shown in the prompt (e.g. `pydrk:/window> `).
Shell commands SHALL reuse the one-shot command set with path arguments
resolved against cwd; absolute paths starting with `/` SHALL be honored as
absolute. `pwd` SHALL print the cwd; `cd <path>` SHALL change it, `cd`
with no argument SHALL go to `/`, `..` SHALL pop one node, and `cd` into a
path that has children but is not itself resolvable SHALL fail with
`node_not_found` leaving cwd unchanged; `exit` (or EOF) SHALL quit the
shell. In the shell, the optional-path grammar of `set`, `get` and
`show` SHALL default to the cwd node, so the workflow `cd` into a node,
`ls` its contents, `show foo` for full property info, `set foo XXX` (or
`set foo 2 XXX` for an array index) works without repeating paths.
Server errors in the shell SHALL print the readable error name and return
to the prompt (the shell SHALL NOT exit on a failed command).

#### Scenario: cd, ls, show, set workflow

- **WHEN** in the shell the user runs `cd /window`, then `cd content`,
  then `ls`, then `show is_visible`, then `set is_visible false`
- **THEN** `ls` lists the content node's children and properties, `show`
  prints the `is_visible` metadata and current value, `set` reports
  success, the app redraws, and a subsequent `get is_visible` prints
  `false`

#### Scenario: shell survives errors

- **WHEN** a shell command fails (e.g. `get no_such_prop`)
- **THEN** the error name is printed and the prompt returns; the shell is
  still usable and cwd is unchanged

### Requirement: Shell tab completion

The interactive shell SHALL provide tab completion (stdlib `readline`):
completing the first word yields command names; completing a later word
that looks like a path yields child node names of the referenced parent,
fetched live from the running app (so newly created `mknode` nodes
complete after creation); completing the first positional argument of
`get`, `set` and `show` yields the cwd node's property names together
with its child node paths (both are valid leading arguments under the
optional-path grammar). Completion SHALL NOT
print duplicates, and completing a partial token SHALL offer all matches
when ambiguous. If `readline` is unavailable the shell SHALL still work
without completion.

#### Scenario: complete a node path

- **WHEN** the user types `cd /win` and presses Tab
- **THEN** the token completes to `/window/`

#### Scenario: complete a property name

- **WHEN** the cwd is a node with an `is_visible` property and the user
  types `set is_v` and presses Tab
- **THEN** the token completes to `is_visible `

#### Scenario: complete a freshly created node

- **WHEN** `mknode /window/content debug_layer layer` was run and the user
  types `cd /window/content/debug` and presses Tab
- **THEN** the token completes to `/window/content/debug_layer/`
