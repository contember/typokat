#!/usr/bin/env python3
"""
Composition-oriented TypeScript program generator for the differential harness.

The grammar is deliberately *narrow and deep*. It does not try to cover TypeScript's
syntax; it composes a handful of constructs — calls, contextually typed callbacks,
arrows, fresh object/array literals, overloads, generics and `this`/class context —
to arbitrary depth. That is the region where `412f321` (backlog `95`) was unsound and
where a hand-written fixture corpus is blind: the trigger shape is

    an argument a contextual re-walk can supersede (arrow / fresh object or array
    literal), nested inside a contextually typed callback, whose value depends on
    that callback's contextually typed parameter.

Every generated program is a tree of small dataclasses, not text. That buys two
things the throwaway fuzzer did not have:

  * **structural shrinking** — the shrinker reduces the tree (drop a statement,
    de-genericise an arrow, drop an overload signature, flatten a nesting level)
    instead of guessing at text, so a failure reduces to a committed minimal repro;
  * **determinism** — `generate(seed, index, ...)` is a pure function of its
    arguments, so "seed 42 file 317" always regenerates byte-identically.

All top-level names are prefixed (`g<seed>_<index>_…`) so a whole batch of generated
programs can be handed to one `tsc` invocation without colliding in the global scope.
The shrinker removes the prefix as its last step, when the repro is a single file.

The generated programs stay within the bounded primitive/array subset also covered by the
test-support ambient unit: numbers, strings, booleans, object literals, and arrays with element
access. Nothing here depends on
`Array.prototype`, `String.prototype`, or any ambient global.
"""

import random
import re
from copy import deepcopy
from dataclasses import dataclass
from typing import Callable, List, Optional, Tuple

# --- types -------------------------------------------------------------------


class Ty:
    """A generated type. Closed set: primitives, object literals, arrays, functions,
    and bare references to a declaration's type parameter."""

    def render(self) -> str:
        raise NotImplementedError


@dataclass(frozen=True)
class Prim(Ty):
    name: str

    def render(self) -> str:
        return self.name


@dataclass(frozen=True)
class Obj(Ty):
    props: Tuple[Tuple[str, Ty], ...]

    def render(self) -> str:
        return "{ " + "; ".join(f"{n}: {t.render()}" for n, t in self.props) + " }"


@dataclass(frozen=True)
class Arr(Ty):
    elem: Ty

    def render(self) -> str:
        inner = self.elem.render()
        # `(() => T)[]` — a bare function type needs parentheses before `[]`.
        return f"({inner})[]" if isinstance(self.elem, Fn) else f"{inner}[]"


@dataclass(frozen=True)
class Fn(Ty):
    params: Tuple[Tuple[str, Ty], ...]
    ret: Ty

    def render(self) -> str:
        ps = ", ".join(f"{n}: {t.render()}" for n, t in self.params)
        return f"({ps}) => {self.ret.render()}"


@dataclass(frozen=True)
class Ref(Ty):
    """A reference to a type parameter in scope, e.g. `T`."""

    name: str

    def render(self) -> str:
        return self.name


NUMBER = Prim("number")
STRING = Prim("string")
BOOLEAN = Prim("boolean")
VOID = Prim("void")

# The primitive a leaf consumer wants, and the one a mismatching value supplies.
LEAF_PRIMS = (NUMBER, STRING)


def other_prim(t: Ty) -> Ty:
    return STRING if t == NUMBER else NUMBER


def literal_of(t: Ty) -> str:
    """A literal expression of type `t` (used for generic seeds and mismatches)."""
    if t == NUMBER:
        return "1"
    if t == STRING:
        return '"s"'
    if t == BOOLEAN:
        return "true"
    if isinstance(t, Obj):
        return "{ " + ", ".join(f"{n}: {literal_of(pt)}" for n, pt in t.props) + " }"
    if isinstance(t, Arr):
        return f"[{literal_of(t.elem)}]"
    raise ValueError(f"no literal for {t.render()}")


# --- expressions / statements ------------------------------------------------


class Node:
    """Base for every mutable AST node the shrinker may replace."""


@dataclass
class Raw(Node):
    """A leaf expression rendered verbatim (`p0`, `p0.a`, `p0[0]`, `p0 + 1`, `1`).

    `alts` are strictly simpler texts the shrinker may substitute — the generator
    knows which simplifications keep the program well-formed, so it records them at
    construction time instead of the shrinker guessing."""

    text: str
    alts: Tuple[str, ...] = ()


@dataclass
class Arrow(Node):
    """`p => body` / `() => body` / `<U,>() => body`."""

    param: Optional[str]
    generic: bool
    body: object  # Block | Raw | Call | ObjLit | ArrLit


@dataclass
class ObjLit(Node):
    props: List[Tuple[str, object]]


@dataclass
class ArrLit(Node):
    items: List[object]


@dataclass
class Call(Node):
    callee: str
    args: List[object]


@dataclass
class Ret(Node):
    """`return <expr>;` — how a contextually typed callback with a non-void return
    type gets its value."""

    expr: object


@dataclass
class Block(Node):
    stmts: List[object]


# --- declarations / program --------------------------------------------------


@dataclass
class Sig:
    typarams: Tuple[str, ...]
    params: Tuple[Tuple[str, Ty], ...]
    ret: Ty


@dataclass
class FuncDecl(Node):
    name: str
    sigs: List[Sig]

    def render(self) -> str:
        out = []
        for s in self.sigs:
            tp = f"<{', '.join(s.typarams)}>" if s.typarams else ""
            ps = ", ".join(f"{n}: {t.render()}" for n, t in s.params)
            out.append(f"declare function {self.name}{tp}({ps}): {s.ret.render()};")
        return "\n".join(out)


@dataclass
class Program:
    decls: List[FuncDecl]
    body: Block
    class_name: Optional[str] = None
    field_name: str = "f"
    origin: str = ""

    def render(self) -> str:
        lines = [d.render() for d in self.decls]
        if self.class_name:
            lines.append(f"class {self.class_name} {{")
            lines.append(f"  {self.field_name}: number = 1;")
            lines.append("  m(): void {")
            lines.extend(render_stmt(s, 2) for s in self.body.stmts)
            lines.append("  }")
            lines.append("}")
        else:
            lines.extend(render_stmt(s, 0) for s in self.body.stmts)
        header = f"// {self.origin}\n" if self.origin else ""
        return header + "\n".join(lines) + "\n"


# --- rendering ---------------------------------------------------------------


def render_stmt(stmt, indent: int) -> str:
    pad = "  " * indent
    if isinstance(stmt, Ret):
        return f"{pad}return {render_expr(stmt.expr, indent)};"
    return f"{pad}{render_expr(stmt, indent)};"


def render_expr(e, indent: int) -> str:
    if isinstance(e, Raw):
        return e.text
    if isinstance(e, Call):
        return e.callee + "(" + ", ".join(render_expr(a, indent) for a in e.args) + ")"
    if isinstance(e, ObjLit):
        if not e.props:
            return "{}"
        return "{ " + ", ".join(f"{n}: {render_expr(v, indent)}" for n, v in e.props) + " }"
    if isinstance(e, ArrLit):
        return "[" + ", ".join(render_expr(i, indent) for i in e.items) + "]"
    if isinstance(e, Arrow):
        # A generic arrow's parameter list must be parenthesized: `<U,>(q) => …` is
        # legal, `<U,>q => …` is a syntax error.
        if e.param is None:
            params = "()"
        elif e.generic:
            params = f"({e.param})"
        else:
            params = e.param
        head = ("<U,>" if e.generic else "") + params
        if isinstance(e.body, Block):
            pad = "  " * indent
            if not e.body.stmts:
                return f"{head} => {{}}"
            inner = "\n".join(render_stmt(s, indent + 1) for s in e.body.stmts)
            return f"{head} => {{\n{inner}\n{pad}}}"
        return f"{head} => {render_expr(e.body, indent)}"
    raise TypeError(f"cannot render {e!r}")


# --- generation --------------------------------------------------------------

# Value types a provider's callback parameter can have. Each one gives the leaf a
# different way to *depend* on the contextual parameter (identity, property access,
# element access, arithmetic).
PROVIDER_VALUE_TYPES = (
    NUMBER,
    STRING,
    Obj((("a", NUMBER),)),
    Obj((("a", STRING), ("b", NUMBER))),
    Arr(NUMBER),
    Arr(STRING),
    Obj((("a", Obj((("b", NUMBER),))),)),
)

PROVIDER_KINDS = ("plain", "plain", "ret", "overload", "generic", "genericArr")
CONSUMER_KINDS = ("fn0", "fn0", "fn1", "obj", "arr", "objfn", "overload", "overloadFn",
                  "generic", "genericObj")


class _Gen:
    """One generation run. Holds the name counter and the emitted declarations."""

    def __init__(self, rng: random.Random, prefix: str, opts: "GenOptions"):
        self.rng = rng
        self.prefix = prefix
        self.opts = opts
        self.decls: List[FuncDecl] = []
        self.counter = 0

    def name(self, stem: str) -> str:
        self.counter += 1
        return f"{self.prefix}{stem}{self.counter}"

    def decl(self, name: str, sigs: List[Sig]) -> str:
        self.decls.append(FuncDecl(name, sigs))
        return name

    # -- values ---------------------------------------------------------------

    def value(self, scope: List[Tuple[str, Ty]], want: Ty) -> Tuple[Raw, Ty]:
        """An expression built from an in-scope contextually typed binding.

        Heavily biased towards *using* the scope: a leaf whose value does not depend
        on an enclosing callback parameter cannot exercise the re-walk trigger at all.
        Returns the expression and its actual type (which may differ from `want` —
        that mismatch is what turns the trigger into a visible diagnostic)."""
        if scope and self.rng.random() < self.opts.scope_bias:
            base, ty = self.rng.choice(scope)
            return self._derive(base, ty)
        # A literal of the wanted type, or of the other one (a deliberate mismatch).
        t = want if self.rng.random() > self.opts.mismatch_prob else other_prim(want)
        return Raw(literal_of(t)), t

    def _derive(self, base: str, ty: Ty) -> Tuple[Raw, Ty]:
        """A projection out of an in-scope binding of type `ty`."""
        choices: List[Tuple[str, Ty, Tuple[str, ...]]] = [(base, ty, ())]
        if isinstance(ty, Obj):
            for n, pt in ty.props:
                choices.append((f"{base}.{n}", pt, (base,)))
                if isinstance(pt, Obj):
                    for n2, pt2 in pt.props:
                        choices.append((f"{base}.{n}.{n2}", pt2, (f"{base}.{n}", base)))
        if isinstance(ty, Arr):
            choices.append((f"{base}[0]", ty.elem, (base,)))
        if ty == NUMBER:
            choices.append((f"{base} + 1", NUMBER, (base,)))
        text, t, alts = self.rng.choice(choices)
        return Raw(text, alts), t

    # -- leaves ---------------------------------------------------------------

    def leaf(self, scope: List[Tuple[str, Ty]]) -> Call:
        """A consumer call whose argument is a form a contextual re-walk can
        supersede: an arrow, a fresh object literal, or a fresh array literal."""
        kind = self.rng.choice(self.opts.consumer_kinds)
        want = self.rng.choice(LEAF_PRIMS)
        gen_arrow = self.rng.random() < self.opts.generic_arrow_prob

        if kind == "fn0":
            name = self.decl(self.name("want"), [Sig((), (("f", Fn((), want)),), VOID)])
            val, _ = self.value(scope, want)
            return Call(name, [Arrow(None, gen_arrow, val)])

        if kind == "fn1":
            pty = self.rng.choice(LEAF_PRIMS)
            name = self.decl(self.name("want"),
                             [Sig((), (("f", Fn((("x", pty),), want)),), VOID)])
            pname = self.name("q")
            val, _ = self.value(scope + [(pname, pty)], want)
            return Call(name, [Arrow(pname, gen_arrow, val)])

        if kind == "obj":
            name = self.decl(self.name("want"),
                             [Sig((), (("o", Obj((("a", want),))),), VOID)])
            val, _ = self.value(scope, want)
            return Call(name, [ObjLit([("a", val)])])

        if kind == "arr":
            name = self.decl(self.name("want"), [Sig((), (("xs", Arr(want)),), VOID)])
            val, _ = self.value(scope, want)
            items = [val]
            if self.rng.random() < 0.3:
                second, _ = self.value(scope, want)
                items.append(second)
            return Call(name, [ArrLit(items)])

        if kind == "objfn":
            name = self.decl(self.name("want"),
                             [Sig((), (("o", Obj((("a", Fn((), want)),))),), VOID)])
            val, _ = self.value(scope, want)
            return Call(name, [ObjLit([("a", Arrow(None, gen_arrow, val))])])

        if kind == "overload":
            name = self.decl(self.name("over"), [
                Sig((), (("o", Obj((("a", STRING),))),), VOID),
                Sig((), (("o", Obj((("a", NUMBER),))),), VOID),
            ])
            val, _ = self.value(scope, want)
            return Call(name, [ObjLit([("a", val)])])

        if kind == "overloadFn":
            name = self.decl(self.name("over"), [
                Sig((), (("f", Fn((), STRING)),), VOID),
                Sig((), (("f", Fn((), NUMBER)),), VOID),
            ])
            val, _ = self.value(scope, want)
            return Call(name, [Arrow(None, gen_arrow, val)])

        if kind == "generic":
            # `shapeOf<T>(shape: T): T` — the real-world zod shape (backlog 95).
            name = self.decl(self.name("shape"), [Sig(("T",), (("shape", Ref("T")),), Ref("T"))])
            val, _ = self.value(scope, want)
            arg = ObjLit([("a", Arrow(None, gen_arrow, val))]) if self.rng.random() < 0.5 \
                else Arrow(None, gen_arrow, val)
            return Call(name, [arg])

        # "genericObj": inference through a fresh object literal.
        name = self.decl(self.name("shape"),
                         [Sig(("T",), (("shape", Obj((("a", Ref("T")),))),), Ref("T"))])
        val, _ = self.value(scope, want)
        return Call(name, [ObjLit([("a", val)])])

    # -- nesting --------------------------------------------------------------

    def nest(self, level: int, depth: int, scope: List[Tuple[str, Ty]]) -> object:
        """One contextually typed callback level: `provider(pN => { … })`.

        The callback parameter is *untyped* at the call site — its type comes from the
        provider's signature, i.e. from contextual typing. That is the binding the
        reverted memo forgot about."""
        kind = self.rng.choice(self.opts.provider_kinds)
        value_ty = self.rng.choice(PROVIDER_VALUE_TYPES)
        pname = f"p{level}"
        ret: Ty = VOID
        extra_args: List[object] = []

        if kind == "plain":
            name = self.decl(self.name("each"),
                             [Sig((), (("step", Fn((("value", value_ty),), VOID)),), VOID)])
        elif kind == "ret":
            ret = self.rng.choice(LEAF_PRIMS)
            name = self.decl(self.name("each"),
                             [Sig((), (("step", Fn((("value", value_ty),), ret)),), VOID)])
        elif kind == "overload":
            alt = self.rng.choice([t for t in PROVIDER_VALUE_TYPES if t != value_ty])
            name = self.decl(self.name("each"), [
                Sig((), (("step", Fn((("value", value_ty),), VOID)),), VOID),
                Sig((), (("step", Fn((("value", alt),), VOID)),), VOID),
            ])
        elif kind == "generic":
            name = self.decl(self.name("each"), [
                Sig(("T",), (("seed", Ref("T")), ("step", Fn((("value", Ref("T")),), VOID))), VOID)
            ])
            extra_args = [Raw(literal_of(value_ty))]
        else:  # "genericArr"
            name = self.decl(self.name("each"), [
                Sig(("T",), (("items", Arr(Ref("T"))), ("step", Fn((("value", Ref("T")),), VOID))),
                    VOID)
            ])
            extra_args = [Raw(f"[{literal_of(value_ty)}]")]

        inner_scope = scope + [(pname, value_ty)]
        stmts: List[object] = []
        n = self.rng.randint(1, self.opts.max_stmts)
        for _ in range(n):
            if level + 1 < depth and self.rng.random() < self.opts.nest_prob:
                stmts.append(self.nest(level + 1, depth, inner_scope))
            else:
                stmts.append(self.leaf(inner_scope))
        if ret != VOID:
            val, _ = self.value(inner_scope, ret)
            stmts.append(Ret(val))
        return Call(name, extra_args + [Arrow(pname, False, Block(stmts))])


@dataclass
class GenOptions:
    """Knobs of the grammar. Defaults are tuned to the region backlog 95 describes."""

    depth_min: int = 1
    depth_max: int = 4
    max_stmts: int = 2
    nest_prob: float = 0.55
    scope_bias: float = 0.9
    mismatch_prob: float = 0.5
    generic_arrow_prob: float = 0.25
    class_prob: float = 0.35
    top_stmts: int = 1
    provider_kinds: Tuple[str, ...] = PROVIDER_KINDS
    consumer_kinds: Tuple[str, ...] = CONSUMER_KINDS


def generate(seed: int, index: int, opts: Optional[GenOptions] = None) -> Program:
    """Generate program `index` of `seed`. Pure: same arguments, same program.

    Seeded from a string: `random.Random(str)` hashes with SHA-512 internally, so the
    stream does not depend on `PYTHONHASHSEED` and a repro regenerates on any host."""
    opts = opts or GenOptions()
    rng = random.Random(f"typokat-differential:{seed}:{index}")
    prefix = f"g{seed}_{index}_"
    g = _Gen(rng, prefix, opts)
    depth = rng.randint(opts.depth_min, opts.depth_max)
    in_class = rng.random() < opts.class_prob
    class_name = f"{prefix}C" if in_class else None
    field_name = "fld"
    scope: List[Tuple[str, Ty]] = [(f"this.{field_name}", NUMBER)] if in_class else []
    stmts = [g.nest(0, depth, scope) for _ in range(opts.top_stmts)]
    return Program(decls=g.decls, body=Block(stmts), class_name=class_name,
                   field_name=field_name,
                   origin=f"differential: seed={seed} index={index} depth={depth}")


# --- shrinking support -------------------------------------------------------
#
# The shrinker (shrink.py) drives these. They live here because they are grammar
# knowledge: which replacements keep a generated program well-formed.

Setter = Callable[[object], None]


def _list_setter(lst: list, i: int) -> Setter:
    def set_(v):
        lst[i] = v

    return set_


def _attr_setter(obj, name: str) -> Setter:
    def set_(v):
        setattr(obj, name, v)

    return set_


def slots(prog: Program) -> List[Tuple[Setter, object]]:
    """Every replaceable slot of the program, in a deterministic DFS order.

    Order is stable for a fixed tree shape, so slot *index* addresses the same node
    in a deep copy — that is how the shrinker applies one reduction at a time
    without mutating the tree it is enumerating."""
    out: List[Tuple[Setter, object]] = []
    for i, d in enumerate(prog.decls):
        out.append((_list_setter(prog.decls, i), d))
    _block_slots(prog.body, _attr_setter(prog, "body"), out)
    return out


def _block_slots(block: Block, setter: Setter, out: List[Tuple[Setter, object]]) -> None:
    out.append((setter, block))
    for i, st in enumerate(block.stmts):
        _expr_slots(st, _list_setter(block.stmts, i), out)


def _expr_slots(e, setter: Setter, out: List[Tuple[Setter, object]]) -> None:
    out.append((setter, e))
    if isinstance(e, Call):
        for i, a in enumerate(e.args):
            _expr_slots(a, _list_setter(e.args, i), out)
    elif isinstance(e, Arrow):
        if isinstance(e.body, Block):
            _block_slots(e.body, _attr_setter(e, "body"), out)
        else:
            _expr_slots(e.body, _attr_setter(e, "body"), out)
    elif isinstance(e, ObjLit):
        for i, (n, v) in enumerate(e.props):
            def mk(idx, key):
                def set_(val):
                    e.props[idx] = (key, val)

                return set_

            _expr_slots(v, mk(i, n), out)
    elif isinstance(e, ArrLit):
        for i, v in enumerate(e.items):
            _expr_slots(v, _list_setter(e.items, i), out)
    elif isinstance(e, Ret):
        _expr_slots(e.expr, _attr_setter(e, "expr"), out)


def uses(node, name: str) -> bool:
    """Whether `name` occurs as an identifier anywhere under `node`."""
    return _ident_re(name).search(_text_of(node)) is not None


def _text_of(node) -> str:
    if isinstance(node, Block):
        return "\n".join(render_stmt(s, 0) for s in node.stmts)
    if isinstance(node, Program):
        return node.render()
    if isinstance(node, FuncDecl):
        return node.render()
    return render_stmt(node, 0)


_IDENT_CACHE: dict = {}


def _ident_re(name: str):
    r = _IDENT_CACHE.get(name)
    if r is None:
        r = re.compile(r"(?<![A-Za-z0-9_$.])" + re.escape(name) + r"(?![A-Za-z0-9_$])")
        _IDENT_CACHE[name] = r
    return r


def simplifications(node) -> List[object]:
    """Strictly smaller replacements for `node` that keep the program well-formed.

    Well-formed does **not** mean type-correct: a reduction that introduces a type
    error is fine, the oracle decides whether the divergence survived."""
    out: List[object] = []
    if isinstance(node, Block):
        for i, st in enumerate(node.stmts):
            out.append(Block(node.stmts[:i] + node.stmts[i + 1:]))
            # Flatten one nesting level: splice a nested callback's body in place of
            # the call, when nothing inside it references the callback's parameter.
            inner = _flattenable(st)
            if inner is not None:
                out.append(Block(node.stmts[:i] + list(inner) + node.stmts[i + 1:]))
    elif isinstance(node, Arrow):
        if node.generic:
            out.append(Arrow(node.param, False, node.body))
        if node.param is not None and not uses(node.body, node.param):
            out.append(Arrow(None, node.generic, node.body))
        if isinstance(node.body, Block) and len(node.body.stmts) == 1 \
                and not isinstance(node.body.stmts[0], Ret):
            out.append(Arrow(node.param, node.generic, node.body.stmts[0]))
    elif isinstance(node, ObjLit):
        if len(node.props) > 1:
            for i in range(len(node.props)):
                out.append(ObjLit(node.props[:i] + node.props[i + 1:]))
    elif isinstance(node, ArrLit):
        if len(node.items) > 1:
            for i in range(len(node.items)):
                out.append(ArrLit(node.items[:i] + node.items[i + 1:]))
    elif isinstance(node, Raw):
        for i, alt in enumerate(node.alts):
            out.append(Raw(alt, node.alts[i + 1:]))
    elif isinstance(node, FuncDecl):
        if len(node.sigs) > 1:
            for i in range(len(node.sigs)):
                out.append(FuncDecl(node.name, node.sigs[:i] + node.sigs[i + 1:]))
    return [deepcopy(o) for o in out]


def _flattenable(stmt) -> Optional[List[object]]:
    """If `stmt` is `provider(… , pN => { S })` and `S` never mentions `pN`, return S."""
    if not isinstance(stmt, Call) or not stmt.args:
        return None
    last = stmt.args[-1]
    if not isinstance(last, Arrow) or not isinstance(last.body, Block):
        return None
    if last.param is not None and uses(last.body, last.param):
        return None
    if any(isinstance(s, Ret) for s in last.body.stmts):
        return None
    return list(last.body.stmts)


def prune_decls(prog: Program) -> Program:
    """Drop declarations the body no longer calls. Applied after every reduction so
    the repro never carries dead lines."""
    body_text = _text_of(prog.body)
    kept = [d for d in prog.decls if _ident_re(d.name).search(body_text)]
    if len(kept) == len(prog.decls):
        return prog
    return Program(kept, prog.body, prog.class_name, prog.field_name, prog.origin)


def drop_class(prog: Program) -> Optional[Program]:
    """Remove the class/`this` wrapper when the body no longer uses `this`."""
    if prog.class_name is None:
        return None
    if "this." in _text_of(prog.body):
        return None
    return Program(prog.decls, prog.body, None, prog.field_name, prog.origin)


def program_reductions(prog: Program):
    """Yield every one-step reduction of `prog`, lazily.

    Lazy on purpose: the shrinker takes the *first* candidate that keeps the
    divergence and restarts, so materialising the whole list would build hundreds of
    deep copies per round for nothing."""
    d = drop_class(prog)
    if d is not None:
        yield prune_decls(d)
    n = len(slots(prog))
    for i in range(n):
        node = slots(prog)[i][1]
        for cand in simplifications(node):
            copy = deepcopy(prog)
            setter, _ = slots(copy)[i]
            setter(cand)
            yield prune_decls(copy)
