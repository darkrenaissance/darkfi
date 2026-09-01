"""Command-line interface for driving a running app's scene graph over
the netdebug ZeroMQ backend. Run from `bin/app` as `python -m pydrk ...`.
With no subcommand an interactive shell is started."""

import argparse
import math
import re
import shlex
import sys

import zmq

from . import exc, serial
from .api import (
    Api,
    CallArgType,
    Expr,
    PropertyStatus,
    PropertySubType,
    PropertyType,
    SceneNodeType,
)
from .print_tree import print_tree
from .vector_shape import VectorShape


class UsageError(Exception):
    pass


PYDRK_ERRORS = tuple(
    obj for obj in vars(exc).values() if isinstance(obj, type) and issubclass(obj, Exception)
)


def error_name(err):
    if isinstance(err, exc.UnknownError):
        return str(err)
    name = type(err).__name__
    name = name.replace("ID", "Id")
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


NODE_TYPE_NAMES = {
    getattr(SceneNodeType, name): name.lower()
    for name in dir(SceneNodeType)
    if name.isupper()
}


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


def format_value(val):
    if val is None:
        return "null"
    if isinstance(val, Expr):
        return f'"{val}"'
    if isinstance(val, bool):
        return "true" if val else "false"
    if isinstance(val, str):
        return f'"{val}"'
    return str(val)


def prop_summary(api, path, prop):
    if prop.type == PropertyType.VECTOR_SHAPE:
        return "<shape>"
    vals = api.get_property_value(path, prop.name)
    formatted = [format_value(v) for v in vals]
    if len(formatted) == 1:
        return formatted[0]
    return "[" + ", ".join(formatted) + "]"


def format_status_value(status, val):
    match status:
        case PropertyStatus.EXPR:
            return f'expr "{val}"'
        case PropertyStatus.NULL:
            return "null"
        case PropertyStatus.UNSET:
            return "unset"
        case _:
            return f"value {format_value(val)}"


def parse_get_args(tokens, cwd):
    tokens = list(tokens)
    idx = None
    if tokens and tokens[-1].isdigit():
        idx = int(tokens.pop())
    if not tokens:
        raise UsageError("usage: get [path] PROP [idx]")
    prop_name = tokens.pop()
    if tokens:
        path = resolve_path(cwd, "/".join(tokens))
    else:
        path = "/" + "/".join(cwd)
    return (path, prop_name, idx)


def parse_show_args(tokens, cwd):
    tokens = list(tokens)
    if not tokens:
        raise UsageError("usage: show [path] PROP")
    prop_name = tokens.pop()
    if tokens:
        path = resolve_path(cwd, "/".join(tokens))
    else:
        path = "/" + "/".join(cwd)
    return (path, prop_name)


def parse_set_args(tokens, cwd):
    tokens = list(tokens)
    if not tokens:
        raise UsageError("usage: set [path] PROP [idx] VAL")
    value = tokens.pop()
    idx = 0
    if tokens and tokens[-1].isdigit():
        idx = int(tokens.pop())
    if not tokens:
        raise UsageError("missing property name in: set [path] PROP [idx] VAL")
    prop_name = tokens.pop()
    if tokens:
        path = resolve_path(cwd, "/".join(tokens))
    else:
        path = "/" + "/".join(cwd)
    return (path, prop_name, idx, value)


def parse_uint32(token):
    try:
        val = int(token, 0)
    except ValueError:
        raise UsageError(f"invalid uint32 value: {token}")
    if not 0 <= val <= 0xFFFFFFFF:
        raise UsageError(f"uint32 value out of range: {token}")
    return val


def encode_set_value(api, path, prop, token, index):
    if token == "null":
        api.set_property_null(path, prop.name, index)
        return
    match prop.type:
        case PropertyType.BOOL:
            if token == "true":
                api.set_property_bool(path, prop.name, index, True)
            elif token == "false":
                api.set_property_bool(path, prop.name, index, False)
            else:
                raise UsageError(f"invalid bool value: {token} (use true/false)")
        case PropertyType.UINT32:
            api.set_property_u32(path, prop.name, index, parse_uint32(token))
        case PropertyType.SCENE_NODE_ID:
            api.set_property_node_id(path, prop.name, index, parse_uint32(token))
        case PropertyType.FLOAT32:
            try:
                val = float(token)
            except ValueError:
                raise UsageError(f"invalid float32 value: {token}")
            api.set_property_f32(path, prop.name, index, val)
        case PropertyType.STR:
            api.set_property_str(path, prop.name, index, token)
        case PropertyType.ENUM:
            if prop.enum_items is None or token not in prop.enum_items:
                raise UsageError(f"invalid enum item: {token} (not in {prop.enum_items})")
            api.set_property_enum(path, prop.name, index, token)
        case _:
            raise UsageError(f"cannot set properties of type {PropertyType.to_str(prop.type)}")


def prop_meta_lines(prop):
    array_len = "unbounded" if prop.array_len == 0 else str(prop.array_len)
    lines = [
        f"{prop.name}:",
        f"  type: {PropertyType.to_str(prop.type)}",
        f"  subtype: {PropertySubType.to_str(prop.subtype)}",
        f"  array_len: {array_len}",
        f"  null_allowed: {'yes' if prop.is_null_allowed else 'no'}",
        f"  expr_allowed: {'yes' if prop.is_expr_allowed else 'no'}",
    ]
    if prop.min_val is not None and prop.max_val is not None:
        lines.append(f"  range: [{format_value(prop.min_val)}, {format_value(prop.max_val)}]")
    if prop.enum_items is not None:
        lines.append(f"  enum_items: [" + ", ".join(prop.enum_items) + "]")
    if prop.ui_name:
        lines.append(f"  ui_name: {prop.ui_name}")
    if prop.desc:
        lines.append(f"  desc: {prop.desc}")
    return lines


def run_command(api, handler, args, cwd):
    try:
        handler(api, args, cwd)
    except UsageError as err:
        print(f"error: {err}", file=sys.stderr)
        sys.exit(1)
    except PYDRK_ERRORS as err:
        print(f"error: {error_name(err)}", file=sys.stderr)
        sys.exit(1)
    except zmq.error.Again:
        print(f"error: no reply from {api.addr}:{api.port}", file=sys.stderr)
        sys.exit(1)
    except zmq.error.ZMQError as err:
        print(f"error: {err}", file=sys.stderr)
        sys.exit(1)


COMMAND_PARSERS = {}

MAIN_PARSER = argparse.ArgumentParser()


SHELL_BUILTINS = ("cd", "pwd", "exit", "quit")


BUILTIN_HELP = {
    "cd": "cd [path]           change the working node (no arg = /, .. pops one)",
    "pwd": "pwd                 print the working node path",
    "exit": "exit | quit         leave the shell (Ctrl-D also works)",
    "quit": "exit | quit         leave the shell (Ctrl-D also works)",
    "help": "help [command]      show overall or per-command help",
}


def print_help(command=None):
    if command is None:
        MAIN_PARSER.print_help()
        print()
        print("shell builtins (interactive mode only):")
        for name in ("cd", "pwd", "exit", "help"):
            print(f"  {BUILTIN_HELP[name]}")
    elif command in COMMAND_PARSERS:
        COMMAND_PARSERS[command].print_help()
    elif command in BUILTIN_HELP:
        print(BUILTIN_HELP[command])
    else:
        raise UsageError(f"unknown command: {command}")


def cmd_help(api, args, cwd):
    print_help(getattr(args, "topic", None))


class ShellExit(Exception):
    pass


class Shell:
    def __init__(self, api):
        self.api = api
        self.cwd = []

    def prompt(self):
        return f"pydrk:/{'/'.join(self.cwd)}> "

    def run(self):
        setup_completion(self)
        while True:
            try:
                line = input(self.prompt())
            except EOFError:
                print()
                return
            except KeyboardInterrupt:
                print()
                continue
            try:
                self.execute(line)
            except ShellExit:
                return
            clear_completion_cache()

    def execute(self, line):
        try:
            tokens = shlex.split(line)
        except ValueError as err:
            print(f"error: {err}", file=sys.stderr)
            return
        if not tokens:
            return

        cmd = tokens[0]
        if cmd in ("exit", "quit"):
            raise ShellExit
        if cmd == "pwd":
            print("/" + "/".join(self.cwd))
            return
        if cmd == "cd":
            self.cd(tokens[1:])
            return

        parser = COMMAND_PARSERS.get(cmd)
        if parser is None:
            print(f"error: unknown command: {cmd}", file=sys.stderr)
            return
        try:
            args = parser.parse_args(tokens[1:])
        except SystemExit:
            return
        try:
            args.func(self.api, args, self.cwd)
        except UsageError as err:
            print(f"error: {err}", file=sys.stderr)
        except PYDRK_ERRORS as err:
            print(f"error: {error_name(err)}", file=sys.stderr)
        except zmq.error.Again:
            print(f"error: no reply from {self.api.addr}:{self.api.port}", file=sys.stderr)
        except zmq.error.ZMQError as err:
            print(f"error: {err}", file=sys.stderr)

    def cd(self, tokens):
        path = resolve_path(self.cwd, tokens[0] if tokens else "/")
        if path != "/":
            parent, _, name = path.rpartition("/")
            try:
                children = self.api.get_children(parent or "/")
            except UsageError as err:
                print(f"error: {err}", file=sys.stderr)
                return
            except PYDRK_ERRORS as err:
                print(f"error: {error_name(err)}", file=sys.stderr)
                return
            except zmq.error.Again:
                print(f"error: no reply from {self.api.addr}:{self.api.port}", file=sys.stderr)
                return
            except zmq.error.ZMQError as err:
                print(f"error: {err}", file=sys.stderr)
                return
            if not any(child_name == name for (child_name, _, _) in children):
                print(f"error: node_not_found: {path}", file=sys.stderr)
                return
        self.cwd = [token for token in path.split("/") if token]


class Completer:
    def __init__(self, shell):
        self.shell = shell
        self.cache = {}

    def clear_cache(self):
        self.cache.clear()

    def cached_children(self, path):
        if path not in self.cache:
            try:
                self.cache[path] = [name for (name, _, _) in self.shell.api.get_children(path)]
            except Exception:
                self.cache[path] = []
        return self.cache[path]

    def cached_props(self, path):
        key = "props:" + path
        if key not in self.cache:
            try:
                self.cache[key] = [prop.name for prop in self.shell.api.get_properties(path)]
            except Exception:
                self.cache[key] = []
        return self.cache[key]

    def path_matches(self, token, text):
        # `token` is the whitespace-delimited word up to the cursor,
        # including any already-typed path components. `text` is what
        # readline wants replaced: readline's default completer delimiters
        # include "/", so text may be only the fragment after the last
        # slash. Matches are returned with the shared head stripped so
        # they align with readline's replacement window.
        strip = len(token) - len(text)
        idx = token.rfind("/")
        if idx == -1:
            head, prefix, prefix_part = "", token, ""
        else:
            head, prefix = token[:idx], token[idx + 1:]
            prefix_part = token[: idx + 1]
        if token.startswith("/"):
            base = resolve_path(self.shell.cwd, head if head else "/")
        else:
            base = resolve_path(self.shell.cwd, head if head else ".")
        return [
            (prefix_part + name + "/")[strip:]
            for name in self.cached_children(base)
            if name.startswith(prefix)
        ]

    def matches(self, text):
        import readline

        buf = readline.get_line_buffer()
        begidx = readline.get_begidx()

        if begidx == 0:
            words = sorted(set(COMMAND_PARSERS) | set(SHELL_BUILTINS))
            return [word for word in words if word.startswith(text)]

        typed = buf[:begidx]
        head_tokens = typed.split()
        cmd = head_tokens[0] if head_tokens else ""
        token = typed[typed.rfind(" ") + 1 :] + text

        if cmd in ("get", "set", "show") and len(head_tokens) == 1:
            cwd_path = "/" + "/".join(self.shell.cwd)
            found = []
            if "/" not in token:
                found += [name + "/" for name in self.cached_children(cwd_path)]
                found += [name + " " for name in self.cached_props(cwd_path)]
                return sorted(set(m for m in found if m.startswith(text)))
            return sorted(set(self.path_matches(token, text)))

        return sorted(set(self.path_matches(token, text)))

    def complete(self, text, state):
        found = self.matches(text)
        return found[state] if state < len(found) else None


_ACTIVE_COMPLETER = None


def setup_completion(shell):
    global _ACTIVE_COMPLETER
    try:
        import readline
    except ImportError:
        return
    readline.parse_and_bind("tab: complete")
    _ACTIVE_COMPLETER = Completer(shell)
    readline.set_completer(_ACTIVE_COMPLETER.complete)


def clear_completion_cache():
    if _ACTIVE_COMPLETER is not None:
        _ACTIVE_COMPLETER.clear_cache()


def shell_main(args):
    api = Api(args.addr, args.port)
    Shell(api).run()


def build_parser():
    global MAIN_PARSER
    endpoint_args = argparse.ArgumentParser(add_help=False)
    endpoint_args.add_argument("--addr", default=argparse.SUPPRESS)
    endpoint_args.add_argument("--port", type=int, default=argparse.SUPPRESS)

    parser = argparse.ArgumentParser(prog="pydrk", description="Drive a running app over the netdebug backend")
    parser.add_argument("--addr", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=9484)
    parser.add_argument("--selftest", action="store_true", help="run built-in checks and exit")

    sub = parser.add_subparsers(dest="command", metavar="<command>")

    p = sub.add_parser("ping", parents=[endpoint_args], help="connectivity check")
    p.set_defaults(func=cmd_ping)

    p = sub.add_parser("ls", parents=[endpoint_args], help="list a node's children and properties")
    p.add_argument("path", nargs="?", default=".")
    p.set_defaults(func=cmd_ls)

    p = sub.add_parser("tree", parents=[endpoint_args], help="recursively print a node's descendants")
    p.add_argument("path", nargs="?", default=".")
    p.add_argument("--depth", type=int, default=None)
    p.set_defaults(func=cmd_tree)

    p = sub.add_parser("props", parents=[endpoint_args], help="list a node's property metadata")
    p.add_argument("path", nargs="?", default=".")
    p.set_defaults(func=cmd_props)

    p = sub.add_parser("get", parents=[endpoint_args], help="print a property's values")
    p.add_argument("positionals", nargs="*", metavar="[path] PROP [idx]")
    p.set_defaults(func=cmd_get)

    p = sub.add_parser("show", parents=[endpoint_args], help="print everything about one property")
    p.add_argument("positionals", nargs="*", metavar="[path] PROP")
    p.set_defaults(func=cmd_show)

    p = sub.add_parser("set", parents=[endpoint_args], help="set a property value")
    p.add_argument("positionals", nargs="*", metavar="[path] PROP [idx] VAL")
    p.add_argument("--expr", action="store_true", help="send VAL as expr source to compile server-side")
    p.set_defaults(func=cmd_set)

    p = sub.add_parser("methods", parents=[endpoint_args], help="list a node's methods")
    p.add_argument("path")
    p.set_defaults(func=cmd_methods)

    p = sub.add_parser("signals", parents=[endpoint_args], help="list a node's signals")
    p.add_argument("path")
    p.set_defaults(func=cmd_signals)

    p = sub.add_parser("mknode", parents=[endpoint_args], help="create and attach a node")
    p.add_argument("parent_path")
    p.add_argument("name")
    p.add_argument("type")
    p.set_defaults(func=cmd_mknode)

    p = sub.add_parser(
        "rmnode",
        parents=[endpoint_args],
        help="remove a node subtree (runtime-only, undone by restarting the app)",
    )
    p.add_argument("path")
    p.set_defaults(func=cmd_rmnode)

    p = sub.add_parser("set-shape", parents=[endpoint_args], help="push vector shape data")
    p.add_argument("path")
    p.add_argument("--prop", default="shape")
    p.add_argument("--index", type=int, default=0)
    p.add_argument(
        "--box",
        nargs=8,
        action=ShapePrimAction,
        metavar=("X1", "Y1", "X2", "Y2", "R", "G", "B", "A"),
    )
    p.add_argument(
        "--gbox",
        nargs=12,
        action=ShapePrimAction,
        metavar=("X1", "Y1", "X2", "Y2", "R", "G", "B", "A", "R", "G", "B", "A"),
    )
    p.add_argument(
        "--vgradient",
        nargs=14,
        action=ShapePrimAction,
        metavar=(
            "X1", "Y1", "X2", "Y2", "R", "G", "B", "A", "R", "G", "B", "A", "STRIPS", "GAMMA",
        ),
    )
    p.add_argument(
        "--outline",
        nargs=9,
        action=ShapePrimAction,
        metavar=("X1", "Y1", "X2", "Y2", "BORDERPX", "R", "G", "B", "A"),
    )
    p.add_argument(
        "--line",
        nargs=9,
        action=ShapePrimAction,
        metavar=("X1", "Y1", "X2", "Y2", "THICKNESS", "R", "G", "B", "A"),
    )
    p.add_argument(
        "--glow",
        nargs=9,
        action=ShapePrimAction,
        metavar=("CX", "CY", "W", "H", "SEGMENTS", "R", "G", "B", "A"),
    )
    p.set_defaults(func=cmd_set_shape)

    p = sub.add_parser("call", parents=[endpoint_args], help="call a node method")
    p.add_argument("path")
    p.add_argument("method")
    p.add_argument("args", nargs="*", metavar="ARG")
    p.set_defaults(func=cmd_call)

    p = sub.add_parser("help", parents=[endpoint_args], help="show overall or per-command help")
    p.add_argument("topic", nargs="?", default=None)
    p.set_defaults(func=cmd_help)

    COMMAND_PARSERS.clear()
    COMMAND_PARSERS.update(sub.choices)
    MAIN_PARSER = parser

    return parser


def cmd_ping(api, args, cwd):
    print(api.hello())


def cmd_ls(api, args, cwd):
    path = resolve_path(cwd, args.path)
    for (name, node_id, node_type) in api.get_children(path):
        print(f"{name} {node_id} {NODE_TYPE_NAMES.get(node_type, '?')}")
    for prop in api.get_properties(path):
        prop_type = PropertyType.to_str(prop.type)
        print(f"{prop.name}: {prop_type} = {prop_summary(api, path, prop)}")


def cmd_tree(api, args, cwd):
    print_tree(api, resolve_path(cwd, args.path), args.depth)


def cmd_props(api, args, cwd):
    path = resolve_path(cwd, args.path)
    for prop in api.get_properties(path):
        for line in prop_meta_lines(prop):
            print(line)


def print_prop_values(api, path, prop_name, idx):
    vals = api.get_property_value_full(path, prop_name)
    for i, (status, val) in enumerate(vals):
        if idx is not None and i != idx:
            continue
        print(f"{i}: {format_status_value(status, val)}")


def cmd_get(api, args, cwd):
    path, prop_name, idx = parse_get_args(args.positionals, cwd)
    print_prop_values(api, path, prop_name, idx)


def find_prop(api, path, prop_name):
    for prop in api.get_properties(path):
        if prop.name == prop_name:
            return prop
    raise exc.PropertyNotFound


def cmd_show(api, args, cwd):
    path, prop_name = parse_show_args(args.positionals, cwd)
    prop = find_prop(api, path, prop_name)
    for line in prop_meta_lines(prop):
        print(line)
    if prop.depends:
        depends = ", ".join(f"({i}, {name})" for (i, name) in prop.depends)
        print(f"  depends: [{depends}]")
    print_prop_values(api, path, prop_name, None)


def cmd_set(api, args, cwd):
    path, prop_name, idx, value = parse_set_args(args.positionals, cwd)
    if args.expr:
        api.set_property_expr(path, prop_name, idx, value)
        return
    prop = find_prop(api, path, prop_name)
    encode_set_value(api, path, prop, value, idx)


def format_signature(method_name, args, results):
    arg_strs = [f"{name}: {CallArgType.to_str(typ)}" for (name, _, typ) in args]
    result_strs = [f"{name}: {CallArgType.to_str(typ)}" for (name, _, typ) in (results or [])]
    return f"{method_name}(" + ", ".join(arg_strs) + ") -> (" + ", ".join(result_strs) + ")"


def cmd_methods(api, args, cwd):
    path = resolve_path(cwd, args.path)
    for method_name in api.get_methods(path):
        method_args, results = api.get_method(path, method_name)
        print(format_signature(method_name, method_args, results))


def cmd_signals(api, args, cwd):
    path = resolve_path(cwd, args.path)
    for sig_name in api.get_signals(path):
        print(sig_name)


NODE_TYPES = {
    "layer": SceneNodeType.LAYER,
    "vector_art": SceneNodeType.VECTOR_ART,
}


def cmd_mknode(api, args, cwd):
    node_type = NODE_TYPES.get(args.type)
    if node_type is None:
        raise UsageError(f"unsupported node type: {args.type} (supported: {', '.join(NODE_TYPES)})")
    parent_path = resolve_path(cwd, args.parent_path)
    node_id = api.add_node(parent_path, args.name, node_type)
    path = parent_path.rstrip("/") + "/" + args.name
    print(f"id={node_id} path={path}")


def cmd_rmnode(api, args, cwd):
    path = resolve_path(cwd, args.path)
    api.remove_node(path)


SHAPE_MAX_VERTS = 65536


class ShapePrimAction(argparse.Action):
    # argparse "append" actions keep one list per flag, losing the order
    # between different flags. This action records (flag, values) pairs in
    # true command-line order so primitives join as given.
    def __call__(self, parser, namespace, values, option_string=None):
        prims = list(getattr(namespace, "prims", []))
        prims.append(((option_string or "").lstrip("-"), values))
        namespace.prims = prims


def coord_arg(token):
    try:
        return float(token)
    except ValueError:
        return token


def num_arg(token, what):
    try:
        return float(token)
    except ValueError:
        raise UsageError(f"invalid {what} value: {token}")


def int_arg(token, what):
    try:
        return int(token, 0)
    except ValueError:
        raise UsageError(f"invalid {what} value: {token}")


def parse_shape_color_args(vals, count):
    if len(vals) != count:
        raise UsageError(f"expected {count} color values (R G B A), got {' '.join(vals)}")
    try:
        return [float(v) for v in vals]
    except ValueError:
        raise UsageError(f"invalid color value: {' '.join(vals)}")


def build_shape(prims):
    shape = VectorShape()
    for (name, vals) in prims:
        match name:
            case "box":
                shape.add_filled_box(
                    coord_arg(vals[0]),
                    coord_arg(vals[1]),
                    coord_arg(vals[2]),
                    coord_arg(vals[3]),
                    parse_shape_color_args(vals[4:], 4),
                )
            case "gbox":
                top = parse_shape_color_args(vals[4:8], 4)
                bottom = parse_shape_color_args(vals[8:], 4)
                shape.add_gradient_box(
                    coord_arg(vals[0]),
                    coord_arg(vals[1]),
                    coord_arg(vals[2]),
                    coord_arg(vals[3]),
                    [top, top, bottom, bottom],
                )
            case "vgradient":
                top = parse_shape_color_args(vals[4:8], 4)
                bottom = parse_shape_color_args(vals[8:12], 4)
                strips = int_arg(vals[12], "strips")
                gamma = num_arg(vals[13], "gamma")
                if strips <= 0:
                    raise UsageError(f"strips must be positive, got {strips}")
                shape.add_smooth_vertical_gradient(
                    coord_arg(vals[0]),
                    coord_arg(vals[1]),
                    coord_arg(vals[2]),
                    coord_arg(vals[3]),
                    top,
                    bottom,
                    strips,
                    gamma,
                )
            case "outline":
                shape.add_outline(
                    coord_arg(vals[0]),
                    coord_arg(vals[1]),
                    coord_arg(vals[2]),
                    coord_arg(vals[3]),
                    coord_arg(vals[4]),
                    parse_shape_color_args(vals[5:], 4),
                )
            case "line":
                coords = []
                for token in vals[:4]:
                    if not isinstance(coord_arg(token), float):
                        raise UsageError(f"line coordinates must be plain numbers, got {token}")
                    coords.append(float(token))
                thickness = num_arg(vals[4], "thickness")
                shape.add_line(
                    coords[0],
                    coords[1],
                    coords[2],
                    coords[3],
                    thickness,
                    parse_shape_color_args(vals[5:], 4),
                )
            case "glow":
                segments = int_arg(vals[4], "segments")
                if segments <= 0:
                    raise UsageError(f"segments must be positive, got {segments}")
                shape.add_radial_glow(
                    coord_arg(vals[0]),
                    coord_arg(vals[1]),
                    coord_arg(vals[2]),
                    coord_arg(vals[3]),
                    segments,
                    0.0,
                    2.0 * math.pi,
                    parse_shape_color_args(vals[5:], 4),
                )
            case _:
                raise UsageError(f"unknown shape primitive: {name}")
    if len(shape.verts) >= SHAPE_MAX_VERTS:
        raise UsageError(
            f"shape has {len(shape.verts)} vertices, exceeding the 16-bit index limit of {SHAPE_MAX_VERTS - 1}"
        )
    return shape


def cmd_set_shape(api, args, cwd):
    prims = getattr(args, "prims", None) or []
    if not prims:
        raise UsageError("no shape primitives given (use --box, --gbox, --vgradient, --outline, --line, --glow)")
    shape = build_shape(prims)
    path = resolve_path(cwd, args.path)
    shape.set(api, path, args.prop, args.index)


def parse_bool(token):
    if token == "true":
        return True
    if token == "false":
        return False
    raise UsageError(f"invalid bool value: {token} (use true/false)")


def encode_call_arg(buf, arg_type, token, arg_name):
    match arg_type:
        case CallArgType.UINT32:
            serial.write_u32(buf, parse_uint32(token))
        case CallArgType.UINT64:
            try:
                val = int(token, 0)
            except ValueError:
                raise UsageError(f"invalid uint64 value for {arg_name}: {token}")
            if not 0 <= val <= 0xFFFFFFFFFFFFFFFF:
                raise UsageError(f"uint64 value out of range for {arg_name}: {token}")
            serial.write_u64(buf, val)
        case CallArgType.FLOAT32:
            serial.write_f32(buf, num_arg(token, arg_name))
        case CallArgType.BOOL:
            serial.write_u8(buf, int(parse_bool(token)))
        case CallArgType.STR:
            serial.encode_str(buf, token)
        case CallArgType.HASH:
            token = token.strip().lower()
            if len(token) != 64:
                raise UsageError(f"invalid hash for {arg_name}: expected 64 hex chars, got {token}")
            try:
                buf += bytes.fromhex(token)
            except ValueError:
                raise UsageError(f"invalid hash hex for {arg_name}: {token}")
        case _:
            raise UsageError(f"unsupported argument type: {CallArgType.to_str(arg_type)}")


CALL_RESULT_SIZES = {
    CallArgType.UINT32: 4,
    CallArgType.UINT64: 8,
    CallArgType.FLOAT32: 4,
    CallArgType.BOOL: 1,
}


def decode_call_result(cur, typ):
    match typ:
        case CallArgType.STR:
            return serial.decode_str(cur)
        case CallArgType.HASH:
            return cur.read(32).hex()
        case _:
            data = cur.read(CALL_RESULT_SIZES[typ])
            return f"0x{data.hex()}"


def cmd_call(api, args, cwd):
    path = resolve_path(cwd, args.path)
    method_args, results = api.get_method(path, args.method)
    if len(args.args) != len(method_args):
        sig = format_signature(args.method, method_args, results)
        raise UsageError(f"wrong number of arguments for {sig}, got {len(args.args)}")
    buf = bytearray()
    for (name, _, typ), token in zip(method_args, args.args):
        encode_call_arg(buf, typ, token, name)
    result = api.call_method(path, args.method, bytes(buf))
    if result is None or not results:
        print("void")
        return
    cur = serial.Cursor(result)
    outs = []
    for (name, _, typ) in results:
        try:
            outs.append(decode_call_result(cur, typ))
        except Exception:
            outs.append(f"0x{cur.remain_data().hex()}")
            break
    print(" ".join(outs))


def run_selftests():
    from .api import Expr, Property, PropertyStatus, PropertyType

    assert format_value(None) == "null"
    assert format_value(Expr("w/2")) == '"w/2"'
    assert format_value(True) == "true"
    assert format_value(False) == "false"
    assert format_value(1.0) == "1.0"
    assert format_value(10) == "10"
    assert format_value("hello world") == '"hello world"'

    assert format_status_value(PropertyStatus.EXPR, Expr("w/2")) == 'expr "w/2"'
    assert format_status_value(PropertyStatus.NULL, None) == "null"
    assert format_status_value(PropertyStatus.UNSET, 1.0) == "unset"
    assert format_status_value(PropertyStatus.OK, 1.0) == "value 1.0"

    assert NODE_TYPE_NAMES[SceneNodeType.LAYER] == "layer"
    assert NODE_TYPE_NAMES[SceneNodeType.VECTOR_ART] == "vector_art"
    assert NODE_TYPE_NAMES[SceneNodeType.PLUGIN_ROOT] == "plugin_root"

    assert resolve_path([], "/") == "/"
    assert resolve_path([], "") == "/"
    assert resolve_path([], "..") == "/"
    assert resolve_path(["a", "b"], "../..") == "/"
    assert resolve_path(["a", "b"], "../../../setting") == "/setting"
    assert resolve_path([], "//window") == "/window"
    assert resolve_path([], "/window/content") == "/window/content"
    assert resolve_path(["window"], "content") == "/window/content"
    assert resolve_path(["window", "content"], "..") == "/window"
    assert resolve_path(["window"], "../setting") == "/setting"
    assert resolve_path(["window"], "./content/.") == "/window/content"
    assert resolve_path([], "window//content/") == "/window/content"

    assert parse_get_args(["alpha"], []) == ("/", "alpha", None)
    assert parse_get_args(["/window/content", "alpha"], []) == ("/window/content", "alpha", None)
    assert parse_get_args(["window", "content", "alpha"], []) == ("/window/content", "alpha", None)
    assert parse_get_args(["rect", "2"], []) == ("/", "rect", 2)
    assert parse_get_args(["rect", "2"], ["window", "content"]) == ("/window/content", "rect", 2)
    for bad in ([], ["2"]):
        try:
            parse_get_args(bad, [])
            raise AssertionError(f"parse_get_args({bad}) should have raised")
        except UsageError:
            pass

    assert parse_show_args(["alpha"], []) == ("/", "alpha")
    assert parse_show_args(["window", "content", "alpha"], []) == ("/window/content", "alpha")
    assert parse_show_args(["alpha"], ["window", "content"]) == ("/window/content", "alpha")

    assert parse_set_args(["is_visible", "false"], []) == ("/", "is_visible", 0, "false")
    assert parse_set_args(["rect", "2", "w/2"], []) == ("/", "rect", 2, "w/2")
    assert parse_set_args(["/window/content", "rect", "2", "w/2"], []) == (
        "/window/content",
        "rect",
        2,
        "w/2",
    )
    assert parse_set_args(["window", "content", "rect", "2", "1.0"], []) == (
        "/window/content",
        "rect",
        2,
        "1.0",
    )
    assert parse_set_args(["rect", "w/2"], ["window"]) == ("/window", "rect", 0, "w/2")
    for bad in ([], ["false"], ["2", "false"]):
        try:
            parse_set_args(bad, [])
            raise AssertionError(f"parse_set_args({bad}) should have raised")
        except UsageError:
            pass

    assert parse_uint32("42") == 42
    assert parse_uint32("0x10") == 16
    for bad in ("-1", "x", "4294967296"):
        try:
            parse_uint32(bad)
            raise AssertionError(f"parse_uint32({bad}) should have raised")
        except UsageError:
            pass

    shape = build_shape([("box", ["0", "0", "w", "10", "1", "0", "0", "1"])])
    assert len(shape.verts) == 4 and len(shape.indices) == 6
    assert shape.verts[1][0] == "w" and shape.verts[3][1] == "10.0"

    shape = build_shape([("gbox", ["0", "0", "w", "h", "1", "1", "1", "1", "0", "0", "0", "1"])])
    assert len(shape.verts) == 4
    assert shape.verts[0][2] == [1.0, 1.0, 1.0, 1.0]
    assert shape.verts[2][2] == [0.0, 0.0, 0.0, 1.0]

    shape = build_shape(
        [("vgradient", ["0", "0", "w", "h", "1", "1", "1", "1", "0", "0", "0", "1", "8", "0.45"])]
    )
    assert len(shape.verts) == 8 * 4 and len(shape.indices) == 8 * 6

    shape = build_shape([("outline", ["0", "0", "w", "h", "2.0", "0", "0", "0", "1"])])
    assert len(shape.verts) == 16 and len(shape.indices) == 24

    shape = build_shape([("line", ["0", "0", "10", "0", "4", "1", "1", "1", "1"])])
    assert len(shape.verts) == 4 and len(shape.indices) == 6

    shape = build_shape([("glow", ["w/2", "h/2", "w", "h", "12", "1", "0", "0", "1"])])
    assert len(shape.verts) == 14 and len(shape.indices) == 36
    assert shape.verts[1][0] == "(w/2 + (w * 0.5))"

    shape = build_shape(
        [
            ("box", ["0", "0", "1", "1", "1", "1", "1", "1"]),
            ("outline", ["0", "0", "1", "1", "1", "0", "0", "0", "1"]),
        ]
    )
    assert len(shape.verts) == 4 + 16

    parser = build_parser()
    assert MAIN_PARSER is parser
    assert "help" in COMMAND_PARSERS
    for name in ("cd", "pwd", "exit", "quit", "help"):
        assert name in BUILTIN_HELP
    assert set(SHELL_BUILTINS) <= set(BUILTIN_HELP)
    args = parser.parse_args(
        ["set-shape", "/x", "--box", "0", "0", "w", "10", "1", "0", "0", "1"]
    )
    assert args.prims == [("box", ["0", "0", "w", "10", "1", "0", "0", "1"])]
    args = parser.parse_args(
        [
            "set-shape", "/x",
            "--box", "0", "0", "1", "1", "1", "1", "1", "1",
            "--outline", "0", "0", "1", "1", "1", "0", "0", "0", "1",
        ]
    )
    assert [name for (name, _) in args.prims] == ["box", "outline"]

    for bad in (
        ["line", ["0", "0", "w", "0", "4", "1", "1", "1", "1"]],
        ["vgradient", ["0", "0", "1", "1", "1", "1", "1", "1", "0", "0", "0", "1", "0", "0.45"]],
        ["glow", ["1", "1", "1", "1", "-3", "1", "0", "0", "1"]],
        ["box", ["0", "0", "1", "1", "x", "0", "0", "1"]],
    ):
        try:
            build_shape([bad])
            raise AssertionError(f"build_shape({bad}) should have raised")
        except UsageError:
            pass

    try:
        build_shape([("vgradient", ["0", "0", "1", "1", "1", "1", "1", "1", "0", "0", "0", "1", "20000", "1"])])
        raise AssertionError("oversized shape should have raised")
    except UsageError as err:
        assert "exceeding" in str(err)

    assert coord_arg("w/2") == "w/2"
    assert coord_arg("3.5") == 3.5

    buf = bytearray()
    encode_call_arg(buf, CallArgType.UINT32, "0x10", "n")
    encode_call_arg(buf, CallArgType.STR, "hello", "s")
    encode_call_arg(buf, CallArgType.HASH, "00" * 32, "h")
    encode_call_arg(buf, CallArgType.BOOL, "true", "b")
    encode_call_arg(buf, CallArgType.FLOAT32, "1.5", "f")
    assert bytes(buf) == bytes.fromhex("10000000") + b"\x05hello" + bytes(32) + b"\x01" + bytes.fromhex(
        "0000c03f"
    )

    cur = serial.Cursor(bytes(buf))
    assert serial.read_u32(cur) == 16
    assert serial.decode_str(cur) == "hello"
    assert cur.read(32) == bytes(32)

    for bad in (
        (CallArgType.BOOL, "yes"),
        (CallArgType.HASH, "1234"),
        (CallArgType.UINT64, "-1"),
    ):
        try:
            encode_call_arg(bytearray(), bad[0], bad[1], "x")
            raise AssertionError(f"encode_call_arg{bad} should have raised")
        except UsageError:
            pass

    prop = Property(
        "alpha",
        PropertyType.FLOAT32,
        0,
        "Alpha",
        "Layer transparency",
        False,
        False,
        1,
        0.0,
        1.0,
        None,
        [],
    )
    lines = prop_meta_lines(prop)
    assert lines[0] == "alpha:"
    assert "  type: float32" in lines
    assert "  array_len: 1" in lines
    assert "  range: [0.0, 1.0]" in lines

    prop = prop._replace(array_len=0, min_val=None, max_val=None)
    assert "  array_len: unbounded" in prop_meta_lines(prop)
    assert not any(line.startswith("  range:") for line in prop_meta_lines(prop))


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.selftest:
        run_selftests()
        print("cli self-test OK")
        return

    if args.command is None:
        shell_main(args)
        return

    api = Api(args.addr, args.port)
    run_command(api, args.func, args, [])


if __name__ == "__main__":
    main()
