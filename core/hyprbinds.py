"""
Your Hyprland keybinds, readable, and editable in your config files.

With a Lua config (hyprland.lua) `hyprctl binds` only says that each bind runs
"__lua" with a number: the number is Hyprland's reference to the dispatcher (or
your function) in its Lua registry.  To show "SUPER + G → Open BROWSER:
zen-browser" two sources are combined:

  * Hyprland itself (read only): the real keys and flags of every bind, and,
    through the Lua registry, the values the dispatcher was made with (the
    command after all the `..`, the workspace a loop produced...) or the
    file:line of a Lua function,
  * your config files: every hl.bind(...) call with its file and line, the
    dispatcher it uses (hl.dsp.window.close...) and the variables in it
    (mainMod, BROWSER...), evaluated as far as plain strings go.

The two lists are matched in order (binds are created in the order the config
runs), so binds made in a loop map onto that loop's hl.bind line.

Editing changes the source text of one hl.bind call (keys, or comments it out,
or appends a new one), keeps a backup in data/hypr-backups, reloads Hyprland and
rolls back if Hyprland reports a config error.  hyprlang configs (bind = ...
lines in .conf files) are listed and can be opened, but not edited here.
"""
import os
import re
import time
import shutil
import logging
from dataclasses import dataclass, field

from core import DATA_DIR, hypr

log = logging.getLogger("hyprbinds")

CONFIG_DIR = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "hypr")
BACKUP_DIR = os.path.join(DATA_DIR, "hypr-backups")
ADDED_MARK = "-- added with Luminescent"

MOD_BITS = [(64, "SUPER"), (4, "CTRL"), (8, "ALT"), (1, "SHIFT"), (2, "CAPS"), (16, "MOD2"), (32, "MOD3"),
            (128, "MOD5")]
_MOD_NAMES = {"SUPER": "SUPER", "WIN": "SUPER", "LOGO": "SUPER", "MOD4": "SUPER", "CTRL": "CTRL",
              "CONTROL": "CTRL", "ALT": "ALT", "MOD1": "ALT", "SHIFT": "SHIFT", "CAPS": "CAPS", "MOD2": "MOD2",
              "MOD3": "MOD3", "MOD5": "MOD5"}

# xkb keycodes (evdev + 8) that configs bind as code:N, shown as the key on the cap
_KEYCODES = {10: "1", 11: "2", 12: "3", 13: "4", 14: "5", 15: "6", 16: "7", 17: "8", 18: "9", 19: "0",
             20: "-", 21: "=", 82: "Num −", 86: "Num +", 63: "Num *", 106: "Num /", 104: "Num Enter"}
_MOUSE = {"mouse:272": "Left mouse", "mouse:273": "Right mouse", "mouse:274": "Middle mouse",
          "mouse:275": "Mouse back", "mouse:276": "Mouse forward", "mouse:277": "Mouse extra 1",
          "mouse:278": "Mouse extra 2", "mouse:279": "Mouse extra 3", "mouse:280": "Mouse extra 4",
          "mouse_up": "Wheel up", "mouse_down": "Wheel down", "mouse_left": "Wheel left", "mouse_right": "Wheel right"}

# dispatcher paths (hl.dsp.<path>) -> what they do
_ACTIONS = {
    "exec_cmd": "Run", "exec_raw": "Run", "exit": "Exit Hyprland", "pass": "Pass key to window",
    "focus": "Focus", "layout": "Layout", "submap": "Submap", "dpms": "Screens on/off", "global": "Global shortcut",
    "send_shortcut": "Send shortcut", "send_key_state": "Send key", "force_idle": "Force idle",
    "window.close": "Close window", "window.kill": "Kill window", "window.float": "Floating",
    "window.fullscreen": "Fullscreen", "window.fullscreen_state": "Fullscreen state", "window.move": "Move window",
    "window.resize": "Resize window", "window.drag": "Drag window (mouse)", "window.cycle_next": "Cycle windows",
    "window.pin": "Pin window", "window.pseudo": "Pseudo-tile", "window.center": "Center window",
    "window.swap": "Swap windows", "window.bring_to_top": "Bring to top", "window.alter_zorder": "Z-order",
    "window.signal": "Signal window", "window.tag": "Tag window", "window.toggle_swallow": "Toggle swallow",
    "workspace.toggle_special": "Toggle special workspace", "workspace.move": "Move workspace",
    "workspace.rename": "Rename workspace", "workspace.swap_monitors": "Swap monitor workspaces",
    "group.toggle": "Toggle group", "group.next": "Next in group", "group.prev": "Previous in group",
    "group.lock": "Lock group", "cursor.move": "Move cursor", "cursor.move_to_corner": "Cursor to corner",
}
_DIRECTIONS = {0: "left", 1: "right", 2: "up", 3: "down"}


# ── a small Lua tokenizer ─────────────────────────────────────────────────────
@dataclass
class Tok:
    kind: str          # name, str, num, op
    text: str
    start: int
    end: int
    line: int
    value: object = None


_LONG_OPEN = re.compile(r"\[(=*)\[")
_NUM = re.compile(r"0[xX][0-9a-fA-F]+|\d+\.?\d*(?:[eE][+-]?\d+)?|\.\d+")
_NAME = re.compile(r"[A-Za-z_]\w*")
_OPS = ("...", "..", "==", "~=", "<=", ">=", "::", "//", "<<", ">>")
_ESC = {"n": "\n", "t": "\t", "r": "\r", "\\": "\\", '"': '"', "'": "'", "a": "\a", "b": "\b", "f": "\f",
        "v": "\v", "0": "\0"}


def tokenize(src: str) -> list[Tok]:
    toks, i, n, line = [], 0, len(src), 1
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if c.isspace():
            i += 1
            continue
        if src.startswith("--", i):
            m = _LONG_OPEN.match(src, i + 2)
            if m:
                close = "]" + m.group(1) + "]"
                j = src.find(close, m.end())
                j = n if j < 0 else j + len(close)
            else:
                j = src.find("\n", i)
                j = n if j < 0 else j
            line += src.count("\n", i, j)
            i = j
            continue
        m = _LONG_OPEN.match(src, i)
        if m:
            close = "]" + m.group(1) + "]"
            j = src.find(close, m.end())
            j = n if j < 0 else j
            body = src[m.end():j].lstrip("\n") if j > m.end() else ""
            toks.append(Tok("str", src[i:j + len(close)], i, j + len(close), line, body))
            line += src.count("\n", i, j + len(close))
            i = j + len(close)
            continue
        if c in "\"'":
            j, out = i + 1, []
            while j < n and src[j] != c and src[j] != "\n":
                if src[j] == "\\" and j + 1 < n:
                    out.append(_ESC.get(src[j + 1], src[j + 1]))
                    j += 2
                    continue
                out.append(src[j])
                j += 1
            toks.append(Tok("str", src[i:j + 1], i, j + 1, line, "".join(out)))
            i = j + 1
            continue
        m = _NUM.match(src, i)
        if m and (c.isdigit() or c == "."):
            toks.append(Tok("num", m.group(), i, m.end(), line, m.group()))
            i = m.end()
            continue
        m = _NAME.match(src, i)
        if m:
            toks.append(Tok("name", m.group(), i, m.end(), line))
            i = m.end()
            continue
        op = next((o for o in _OPS if src.startswith(o, i)), c)
        toks.append(Tok("op", op, i, i + len(op), line))
        i += len(op)
    return toks


_OPENERS = {"function", "if", "do", "repeat"}       # 'for'/'while' open with their 'do'
_LOOPS = {"for", "while", "repeat"}


@dataclass
class Call:
    """One hl.bind(...) in a config file."""
    file: str
    line: int
    end_line: int
    start: int                      # offsets of the whole call in the file
    end: int
    args: list[list[Tok]]           # tokens of each argument
    src: str                        # the file's text when parsed
    in_loop: bool
    local_vars: dict
    funcs: dict = field(default_factory=dict)
    loop: dict | None = None        # {"var", "start", "locals": [(name, tokens)]} of the innermost for loop
    count: int = 0                  # binds matched to it so far (the loop's iteration)

    def env(self, iteration: int = 0) -> dict:
        """Variables as they are when the bind runs (for a loop: in its `iteration`-th pass)."""
        env = {k: v for k, v in self.local_vars.items()}
        for k, v in list(env.items()):
            try:
                env[k] = float(v) if re.fullmatch(r"-?\d+(\.\d+)?", str(v)) else v
            except ValueError:
                pass
        if self.loop:
            start = eval_expr(self.loop["start"], env, self.funcs)
            if isinstance(start, float):
                env[self.loop["var"]] = start + iteration
            for name, toks in self.loop["locals"]:
                env[name] = eval_expr(toks, env, self.funcs)
        return env

    def keys_text(self, iteration: int = 0) -> str | None:
        v = eval_expr(self.args[0], self.env(iteration), self.funcs) if self.args else None
        return v if isinstance(v, str) else None

    def arg_text(self, i: int) -> str:
        a = self.args[i] if i < len(self.args) else []
        return self.src[a[0].start:a[-1].end] if a else ""

    def arg_span(self, i: int) -> tuple[int, int]:
        a = self.args[i]
        return a[0].start, a[-1].end


@dataclass
class ParsedFile:
    path: str
    calls: list[Call]
    variables: dict          # top-level NAME = "string" assignments (globals and locals)
    globals_: dict


def _expr_end(toks: list[Tok], i: int) -> int:
    """End of a simple expression (strings, names, numbers, .., calls) starting at toks[i]."""
    depth = 0
    j = i
    while j < len(toks):
        t = toks[j]
        if t.kind == "op" and t.text in "([{":
            depth += 1
        elif t.kind == "op" and t.text in ")]}":
            if depth == 0:
                break
            depth -= 1
        elif depth == 0:
            if t.kind == "op" and t.text not in ("..", ".", ":"):
                break
            if t.kind == "name" and t.text in ("local", "function", "end", "if", "for", "while", "return",
                                               "then", "do", "else", "elseif"):
                break
            if j > i and t.kind in ("name", "str", "num") and toks[j - 1].kind in ("name", "str", "num") \
                    and toks[j - 1].text not in (".", ":"):
                break                    # the next statement
        j += 1
    return j


def evaluate(toks: list[Tok], variables: dict) -> tuple[list, list[str]]:
    """Concatenation parts of an expression: strings, or None where it can't be worked out statically.
    Also returns the variable names it uses."""
    parts, names, cur, depth = [], [], [], 0
    for t in toks + [Tok("op", "..", 0, 0, 0)]:
        if t.kind == "op" and t.text in "([{":
            depth += 1
        elif t.kind == "op" and t.text in ")]}":
            depth -= 1
        if depth == 0 and t.kind == "op" and t.text == "..":
            if len(cur) == 1 and cur[0].kind == "str":
                parts.append(cur[0].value)
            elif len(cur) == 1 and cur[0].kind == "num":
                parts.append(cur[0].text)
            elif len(cur) == 1 and cur[0].kind == "name" and cur[0].text in ("true", "false"):
                parts.append(cur[0].text)
            elif len(cur) == 1 and cur[0].kind == "name":
                names.append(cur[0].text)
                v = variables.get(cur[0].text)
                parts.append(v if isinstance(v, str) else None)
            else:
                names += [x.text for x in cur if x.kind == "name"]
                parts.append(None)
            cur = []
            continue
        cur.append(t)
    return parts, names


class _Eval:
    """Evaluates the simple Lua expressions configs use for keys: strings, numbers, .., arithmetic,
    comparisons, and/or, and calls to small helpers like `local function digitCode(d) return ... end`.
    Anything else raises ValueError."""
    _PREC = [("or",), ("and",), ("==", "~=", "<", ">", "<=", ">="), ("..",), ("+", "-"), ("*", "/", "%", "//")]

    def __init__(self, toks: list[Tok], env: dict, funcs: dict, depth: int = 0):
        self.t, self.i, self.env, self.funcs, self.depth = toks, 0, env, funcs, depth

    def run(self):
        v = self.expr(0)
        if self.i != len(self.t):
            raise ValueError("trailing tokens")
        return v

    def _peek(self) -> Tok | None:
        return self.t[self.i] if self.i < len(self.t) else None

    def _is(self, *texts) -> bool:
        p = self._peek()
        return p is not None and p.kind in ("op", "name") and p.text in texts

    def expr(self, level: int):
        if level == len(self._PREC):
            return self.unary()
        left = self.expr(level + 1)
        ops = self._PREC[level]
        while self._is(*ops):
            op = self.t[self.i].text
            self.i += 1
            right = self.expr(level if op == ".." else level + 1)
            left = self._apply(op, left, right)
        return left

    @staticmethod
    def _apply(op, a, b):
        if op == "or":
            return a if a not in (None, False) else b
        if op == "and":
            return b if a not in (None, False) else a
        if op == "==":
            return a == b
        if op == "~=":
            return a != b
        if op == "..":
            return _lua_str(a) + _lua_str(b)
        a, b = float(a), float(b)
        r = {"<": a < b, ">": a > b, "<=": a <= b, ">=": a >= b, "+": a + b, "-": a - b, "*": a * b,
             "/": a / b, "%": a % b, "//": a // b}[op]
        return r

    def unary(self):
        if self._is("-"):
            self.i += 1
            return -float(self.unary())
        if self._is("not"):
            self.i += 1
            return self.unary() in (None, False)
        return self.atom()

    def atom(self):
        t = self._peek()
        if t is None:
            raise ValueError("end of expression")
        self.i += 1
        if t.kind == "str":
            return t.value
        if t.kind == "num":
            return float(t.text) if not t.text.lower().startswith("0x") else float(int(t.text, 16))
        if t.kind == "op" and t.text == "(":
            v = self.expr(0)
            if not self._is(")"):
                raise ValueError("missing )")
            self.i += 1
            return v
        if t.kind == "name":
            if t.text in ("true", "false", "nil"):
                return {"true": True, "false": False, "nil": None}[t.text]
            if self._is("("):
                return self.call(t.text)
            if t.text in self.env:
                return self.env[t.text]
        raise ValueError(f"can't evaluate {t.text}")

    def call(self, name: str):
        fn = self.funcs.get(name)
        if fn is None or self.depth > 8:
            raise ValueError(f"unknown function {name}")
        self.i += 1                                    # (
        args, cur, depth = [], [], 0
        while self.i < len(self.t):
            x = self.t[self.i]
            if x.kind == "op" and x.text in "([{":
                depth += 1
            elif x.kind == "op" and x.text in ")]}":
                if depth == 0:
                    break
                depth -= 1
            if x.kind == "op" and x.text == "," and depth == 0:
                args.append(cur)
                cur = []
            else:
                cur.append(x)
            self.i += 1
        if cur:
            args.append(cur)
        self.i += 1                                    # )
        params, body = fn
        env = dict(self.env)
        for p, a in zip(params, args):
            env[p] = _Eval(a, self.env, self.funcs, self.depth + 1).run()
        return _Eval(body, env, self.funcs, self.depth + 1).run()


def _lua_str(v) -> str:
    if isinstance(v, float) and v.is_integer():
        return str(int(v))
    if isinstance(v, bool) or v is None:
        raise ValueError("can't concatenate")
    return str(v)


def eval_expr(toks: list[Tok], env: dict, funcs: dict):
    """The value of an expression, or None if it can't be worked out from the file alone."""
    try:
        return _Eval(toks, env, funcs).run()
    except (ValueError, KeyError, TypeError, ZeroDivisionError, RecursionError):
        return None


def _join(parts: list) -> str | None:
    return None if any(p is None for p in parts) else "".join(parts)


def parse_file(path: str, inherited: dict | None = None) -> ParsedFile:
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        src = f.read()
    toks = tokenize(src)
    variables = dict(inherited or {})
    globals_: dict = {}
    calls: list[Call] = []
    funcs: dict = {}
    stack: list[str] = []
    loops: list[dict | None] = []          # parallel to stack: loop info for "for" entries
    i = 0
    while i < len(toks):
        t = toks[i]
        if t.kind == "name":
            if t.text in ("for", "while"):
                info = None
                # for NAME = start, stop do
                if t.text == "for" and i + 2 < len(toks) and toks[i + 1].kind == "name" and toks[i + 2].text == "=":
                    j, depth = i + 3, 0
                    while j < len(toks) and not (depth == 0 and toks[j].text == ","):
                        depth += toks[j].text in "([{" and toks[j].kind == "op"
                        depth -= toks[j].text in ")]}" and toks[j].kind == "op"
                        j += 1
                    info = {"var": toks[i + 1].text, "start": toks[i + 3:j], "locals": []}
                stack.append(t.text)
                loops.append(info)
                # the 'do' that follows belongs to this loop
                j = i + 1
                while j < len(toks) and not (toks[j].kind == "name" and toks[j].text == "do"):
                    j += 1
                i = j + 1
                continue
            if t.text == "function" or (t.text == "local" and i + 1 < len(toks) and toks[i + 1].text == "function"):
                # local function NAME(params) return EXPR end  -> a helper keys can be worked out with
                k = i + 2 if t.text == "local" else i + 1
                if k + 1 < len(toks) and toks[k].kind == "name" and toks[k + 1].text == "(":
                    j = k + 2
                    params = []
                    while j < len(toks) and toks[j].text != ")":
                        if toks[j].kind == "name":
                            params.append(toks[j].text)
                        j += 1
                    if j + 1 < len(toks) and toks[j + 1].text == "return":
                        e = j + 2
                        depth = 0
                        while e < len(toks) and not (depth == 0 and toks[e].kind == "name" and toks[e].text == "end"):
                            depth += toks[e].kind == "op" and toks[e].text in "([{"
                            depth -= toks[e].kind == "op" and toks[e].text in ")]}"
                            e += 1
                        funcs[toks[k].text] = (params, toks[j + 2:e])
                if t.text == "local":
                    i += 1
                    continue
            if t.text in _OPENERS:
                stack.append(t.text)
                loops.append(None)
            elif t.text == "end" or t.text == "until":
                if stack:
                    stack.pop()
                    loops.pop()
            elif stack and stack[-1] == "for" and loops[-1] is not None and t.text == "local" \
                    and i + 2 < len(toks) and toks[i + 1].kind == "name" and toks[i + 2].text == "=":
                end = _expr_end(toks, i + 3)
                loops[-1]["locals"].append((toks[i + 1].text, toks[i + 3:end]))
            elif not stack or all(s == "do" for s in stack):
                # NAME = expr / local NAME = expr at file level
                is_local = t.text == "local"
                k = i + 1 if is_local else i
                if k + 2 < len(toks) and toks[k].kind == "name" and toks[k + 1].text == "=" \
                        and toks[k + 1].kind == "op" and (i == 0 or toks[i - 1].text not in (".", ":")):
                    end = _expr_end(toks, k + 2)
                    parts, _ = evaluate(toks[k + 2:end], variables)
                    val = _join(parts)
                    if val is None and end == k + 3 and toks[k + 2].kind == "num":
                        val = toks[k + 2].text
                    if val is not None:
                        variables[toks[k].text] = val
                        if not is_local:
                            globals_[toks[k].text] = val
        if (t.kind == "name" and t.text == "hl" and i + 3 < len(toks) and toks[i + 1].text == "."
                and toks[i + 2].text == "bind" and toks[i + 3].text == "("
                and (i == 0 or toks[i - 1].text not in (".", ":"))):
            depth, j, args, cur, blocks = 0, i + 4, [], [], 0
            while j < len(toks):
                x = toks[j]
                if x.kind == "op" and x.text in "([{":
                    depth += 1
                elif x.kind == "op" and x.text in ")]}":
                    if depth == 0 and x.text == ")":
                        break
                    depth -= 1
                elif x.kind == "name" and x.text in ("function", "if", "do", "repeat"):
                    blocks += 1
                elif x.kind == "name" and x.text in ("end", "until"):
                    blocks -= 1
                if x.kind == "op" and x.text == "," and depth == 0 and blocks == 0:
                    args.append(cur)
                    cur = []
                else:
                    cur.append(x)
                j += 1
            if cur:
                args.append(cur)
            if j < len(toks):
                loop = next((info for info in reversed(loops) if info), None)
                calls.append(Call(path, t.line, toks[j].line, t.start, toks[j].end, args, src,
                                  any(s in _LOOPS for s in stack), dict(variables), funcs, loop))
                i = j + 1
                continue
        i += 1
    return ParsedFile(path, calls, variables, globals_)


def config_files() -> list[str]:
    """The Lua config files in the order Hyprland runs them (hyprland.lua, then its require()s)."""
    main = os.path.join(CONFIG_DIR, "hyprland.lua")
    if not os.path.isfile(main):
        return []
    order, seen = [], set()

    def visit(path):
        if path in seen or not os.path.isfile(path):
            return
        seen.add(path)
        order.append(path)
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return
        for m in re.finditer(r"""^[^-\n]*\brequire\s*\(?\s*["']([\w./-]+)["']""", text, re.M):
            mod = m.group(1)
            visit(os.path.join(CONFIG_DIR, *mod.split(".")) + ".lua")
    visit(main)
    for root, _dirs, files in os.walk(CONFIG_DIR):            # anything loaded some other way
        for name in sorted(files):
            p = os.path.join(root, name)
            if name.endswith(".lua") and p not in seen:
                order.append(p)
                seen.add(p)
    return order


def parse_config() -> list[ParsedFile]:
    files, glob = [], {}
    for path in config_files():
        try:
            pf = parse_file(path, glob)
        except OSError:
            continue
        glob.update(pf.globals_)
        files.append(pf)
    # globals set in a later file (variables.lua) are visible to binds in an earlier one at run time
    for pf in files:
        for c in pf.calls:
            for k, v in glob.items():
                c.local_vars.setdefault(k, v)
    return files


# ── what Hyprland knows ───────────────────────────────────────────────────────
_DUMP = r"""
local function ser(v, d)
  d = d or 0
  local t = type(v)
  if t == "string" then return string.format("%q", v) end
  if t == "number" or t == "boolean" or t == "nil" then return tostring(v) end
  if t == "table" and d < 2 then
    local parts = {}
    for k, x in pairs(v) do parts[#parts + 1] = tostring(k) .. "=" .. ser(x, d + 1) end
    return "{" .. table.concat(parts, ",") .. "}"
  end
  return "<" .. t .. ">"
end
local reg = debug.getregistry()
local out = {}
for _, a in ipairs(LUMI_ARGS) do
  local ok, line = pcall(function()
    local f = reg[a]
    if type(f) ~= "function" then return tostring(a) .. "\t?" end
    local i = debug.getinfo(f, "S")
    if i.what == "C" then
      local ups = {}
      for n = 1, 8 do
        local name, v = debug.getupvalue(f, n)
        if name == nil then break end
        ups[#ups + 1] = ser(v)
      end
      return tostring(a) .. "\tC\t" .. table.concat(ups, "\t")
    end
    return tostring(a) .. "\tL\t" .. i.source .. "\t" .. tostring(i.linedefined)
  end)
  out[#out + 1] = ok and line or (tostring(a) .. "\t?")
end
LUMI_ARGS = nil
return table.concat(out, "\n")
"""


def _lua_values(text: str) -> list:
    """Parse the dumped upvalues ('"m~3"', '2.0', 'nil', '{...}') into Python values (strings stay strings)."""
    out = []
    for part in text.split("\t"):
        part = part.strip()
        if not part or part == "nil":
            out.append(None)
        elif part.startswith('"'):
            toks = tokenize(part)
            out.append(toks[0].value if toks else part.strip('"'))
        elif part in ("true", "false"):
            out.append(part == "true")
        else:
            try:
                out.append(float(part))
            except ValueError:
                out.append(part)
    return out


def runtime_binds() -> list[dict]:
    """Hyprland's binds (j/binds), each with 'lua' = {'kind': 'C'|'L', 'values': [...], 'source', 'line'}."""
    binds = hypr.query("binds") or []
    args = [b.get("arg", "") for b in binds if b.get("dispatcher") == "__lua" and str(b.get("arg", "")).isdigit()]
    info: dict[str, dict] = {}
    if args:
        out = hypr.request(f"repl LUMI_ARGS = {{{','.join(args)}}} " + _DUMP.replace("\n", " "), timeout=3)
        for row in out.splitlines():
            cols = row.split("\t")
            if len(cols) < 2:
                continue
            if cols[1] == "C":
                info[cols[0]] = {"kind": "C", "values": _lua_values("\t".join(cols[2:]))}
            elif cols[1] == "L" and len(cols) >= 4:
                src = cols[2][1:] if cols[2].startswith("@") else cols[2]
                info[cols[0]] = {"kind": "L", "source": src, "line": int(cols[3]) if cols[3].isdigit() else 0}
    for b in binds:
        b["lua"] = info.get(str(b.get("arg", "")))
    return binds


# ── combining them ────────────────────────────────────────────────────────────
@dataclass
class Bind:
    keys: str                      # "SUPER + SHIFT + 1"
    mods: tuple                    # ("SUPER", "SHIFT")
    key: str                       # Hyprland's key name ("code:10", "Q")
    action: str                    # "Open BROWSER", "Close window"
    detail: str = ""               # "zen-browser", "left"
    origin: str = "hyprland"       # hyprland | luminescent
    owner: str = ""                # Luminescent: the module that owns it
    file: str = ""
    line: int = 0
    call: Call | None = None
    flags: list = field(default_factory=list)
    release: bool = False
    conflict: bool = False
    combo: str = ""                # Luminescent: its combo as Luminescent writes it ("ctrl+alt+k")

    @property
    def editable(self) -> bool:
        return self.call is not None and not self.call.in_loop

    def combo_id(self) -> tuple:
        return tuple(sorted(self.mods)), self.key.lower(), self.release


def key_label(key: str) -> str:
    if key in _MOUSE:
        return _MOUSE[key]
    m = re.fullmatch(r"code:(\d+)", key)
    if m and int(m.group(1)) in _KEYCODES:
        return _KEYCODES[int(m.group(1))]
    return key


def mods_of(mask: int) -> tuple:
    return tuple(name for bit, name in MOD_BITS if mask & bit)


def _static_keys(call: Call, iteration: int = 0) -> tuple[set, str | None] | None:
    """(modifiers, key or None for "worked out at run time") of a call's first argument."""
    if not call.args:
        return None
    text = call.keys_text(iteration)
    if text is None:
        parts, _ = evaluate(call.args[0], call.local_vars)
        text = "".join("\0" if p is None else p for p in parts)
    tokens = [x.strip() for x in text.split("+") if x.strip()]
    mods, key = set(), None
    for x in tokens:
        if x.upper() in _MOD_NAMES:
            mods.add(_MOD_NAMES[x.upper()])
        elif "\0" in x:
            key = None
        else:
            key = x
    return mods, key


def _matches(call: Call, b: dict) -> bool:
    sk = _static_keys(call, call.count)
    if sk is None:
        return False
    mods, key = sk
    if set(mods_of(int(b.get("modmask", 0)))) != mods:
        return False
    rk = str(b.get("key", ""))
    # Hyprland's JSON has no key for code:N binds; the config does
    return key is None or not rk or key.lower() == rk.lower()


def _dispatcher(call: Call) -> tuple[str, list[Tok]]:
    """('window.close', tokens of its arguments) for hl.dsp.window.close(...), ('function', body) for a function."""
    a = call.args[1] if len(call.args) > 1 else []
    if a and a[0].kind == "name" and a[0].text == "function":
        return "function", a
    if len(a) >= 4 and a[0].text == "hl" and a[2].text == "dsp":
        path, j = [], 4
        while j < len(a) and a[j].kind == "name":
            path.append(a[j].text)
            if j + 1 < len(a) and a[j + 1].text == ".":
                j += 2
                continue
            j += 1
            break
        inner = a[j + 1:-1] if j < len(a) and a[j].text == "(" else []
        return ".".join(path), inner
    if len(a) == 1 and a[0].kind == "name":
        return "name:" + a[0].text, a
    return "", a


def _table_text(toks: list[Tok], variables: dict, runtime: list) -> str:
    """'{ direction = "left" }' -> 'left'; values a loop works out come from what Hyprland stored."""
    if not toks:
        return ""
    if toks[0].text == "{":
        toks = toks[1:-1]
    items, cur, depth = [], [], 0
    for t in toks + [Tok("op", ",", 0, 0, 0)]:
        if t.kind == "op" and t.text in "([{":
            depth += 1
        elif t.kind == "op" and t.text in ")]}":
            depth -= 1
        if depth == 0 and t.kind == "op" and t.text == ",":
            if cur:
                items.append(cur)
            cur = []
        else:
            cur.append(t)
    rt = [v for v in runtime if isinstance(v, str)]
    shown = []
    for it in items:
        name = it[0].text if len(it) > 2 and it[1].text == "=" else ""
        expr = it[2:] if name else it
        val = _join(evaluate(expr, variables)[0])
        if val is None:
            val = rt.pop(0) if rt else "…"
        if name in ("action",) and val == "toggle":
            continue
        if val == "":
            val = "(not set)"
        shown.append(val if name in ("direction", "mode", "") else f"{name} {val}")
    return ", ".join(shown)


def describe(call: Call | None, b: dict) -> tuple[str, str]:
    """(action, detail) for one bind."""
    lua = b.get("lua") or {}
    values = lua.get("values") or []
    if b.get("dispatcher") not in ("__lua", None):
        # a hyprlang config: already readable
        d = str(b.get("dispatcher", ""))
        return ("Run" if d == "exec" else d.replace("_", " ").capitalize()), str(b.get("arg", ""))
    if call is None:
        if lua.get("kind") == "L":
            return "Lua function", f"{os.path.basename(lua.get('source', ''))}:{lua.get('line', 0)}"
        strings = [v for v in values if isinstance(v, str)]
        return "Lua bind", " ".join(strings)
    kind, inner = _dispatcher(call)
    if kind == "function":
        body = call.arg_text(1)
        body = re.sub(r"^function\s*\([^)]*\)\s*", "", body)
        body = re.sub(r"\s*end$", "", body)
        return "Lua", " ".join(body.split())[:120]
    if kind.startswith("name:"):
        return "Lua", kind[5:] + "()"
    if kind in ("exec_cmd", "exec_raw"):
        cmd = next((v for v in values if isinstance(v, str)), None)
        parts, names = evaluate(inner, call.local_vars)
        if cmd is None:
            cmd = _join(parts) or call.src[inner[0].start:inner[-1].end] if inner else ""
        shown = re.sub(r"^(uwsm app --|uwsm-app --|app2unit --)\s*", "", cmd)
        caps = [n for n in names if n.isupper() and len(n) > 1 and isinstance(call.local_vars.get(n), str)]
        if caps:
            return f"Open {caps[-1]}", shown
        prefix = next((n for n in names if not n.isupper()), "")
        if prefix and "noct" in prefix.lower() and shown.startswith("noctalia msg "):
            return "Noctalia", shown[len("noctalia msg "):]
        return "Run", shown
    label = _ACTIONS.get(kind, kind.replace(".", " ").replace("_", " ").capitalize() if kind else "Lua bind")
    detail = _table_text(inner, call.local_vars, values)
    detail = {"u": "up", "d": "down", "l": "left", "r": "right"}.get(detail, detail)
    detail = detail.replace("follow false", "don't follow")
    if kind == "window.fullscreen":
        detail = {"1": "maximise", "0": "fullscreen", "": "fullscreen"}.get(detail, detail)
    if kind == "focus" and not detail:
        detail = _DIRECTIONS.get(int(values[0]), "") if values and isinstance(values[0], float) else ""
    return label, detail


def _flags(b: dict) -> list[str]:
    names = [("repeat", "repeats"), ("locked", "locked"), ("release", "on release"),
             ("non_consuming", "passes keys"), ("longPress", "long press"), ("mouse", "mouse")]
    return [label for k, label in names if b.get(k)]


def load(luminescent: list[tuple[str, str, str]] | None = None) -> list[Bind]:
    """Every bind: Hyprland's (from your config) and, if given, Luminescent's [(combo, owner, label)]."""
    files = parse_config()
    calls = [c for pf in files for c in pf.calls]
    rows: list[Bind] = []
    ci = 0
    for b in runtime_binds():
        if b.get("description") == hypr.BIND_DESC:
            continue                                    # listed from Luminescent's own registry below
        mods = mods_of(int(b.get("modmask", 0)))
        key = str(b.get("key") or (f"code:{b.get('keycode')}" if b.get("keycode") else ""))
        call = None
        if b.get("dispatcher") == "__lua":
            lua = b.get("lua") or {}
            if lua.get("kind") == "L" and lua.get("source"):
                # a Lua function knows its own file and line
                call = next((c for c in calls if c.file == lua["source"] and c.line <= lua["line"] <= c.end_line),
                            None)
            if call is None:
                # binds are created in config order: try the current call, then the next few
                for j in range(ci, min(ci + 6, len(calls))):
                    if _matches(calls[j], b):
                        call, ci = calls[j], j if calls[j].in_loop else j + 1
                        break
            if call is not None:
                sk = _static_keys(call, call.count)
                if not key and sk and sk[1]:
                    key = sk[1]
                call.count += 1
            elif call in calls:
                ci = calls.index(call) + 1
        action, detail = describe(call, b)
        row = Bind(" + ".join(list(mods) + [key_label(key)]), mods, key, action, detail,
                   file=call.file if call else "", line=call.line if call else 0, call=call,
                   flags=_flags(b), release=bool(b.get("release")))
        if not row.file and b.get("dispatcher") != "__lua":
            row.file, row.line = find_conf_line(key, mods, str(b.get("arg", "")))
        rows.append(row)
    for combo, owner, label in luminescent or []:
        for keys in _lum_keys(combo):
            parts = [p.strip() for p in keys.split("+")]
            mods = tuple(_MOD_NAMES.get(p.upper(), p.upper()) for p in parts[:-1])
            rows.append(Bind(" + ".join(list(mods) + [key_label(parts[-1])]), mods, parts[-1], label,
                             "", origin="luminescent", owner=owner,
                             release=combo in ("alt", "ctrl", "shift", "windows"), combo=combo))
    seen: dict[tuple, list[Bind]] = {}
    for r in rows:
        seen.setdefault(r.combo_id(), []).append(r)
    for group in seen.values():
        if len(group) > 1:
            for r in group:
                r.conflict = True
    return rows


def _lum_keys(combo: str) -> list[str]:
    from core import hotkeys
    return hotkeys.to_hyprland(combo)


def find_conf_line(key: str, mods: tuple, arg: str) -> tuple[str, int]:
    """hyprlang configs: the best matching 'bind... =' line."""
    best, score_best = ("", 0), 0
    key_re = re.compile(r"(?<![\w])" + re.escape(key) + r"(?![\w])", re.I)
    for root, _dirs, files in os.walk(CONFIG_DIR):
        for name in files:
            if not name.endswith(".conf"):
                continue
            path = os.path.join(root, name)
            try:
                lines = open(path, encoding="utf-8", errors="ignore").read().splitlines()
            except OSError:
                continue
            for i, line in enumerate(lines, 1):
                s = line.strip()
                if not s.startswith("bind") or not key_re.search(s):
                    continue
                score = 1 + sum(1 for m in mods if m.lower() in s.lower()) + (3 if arg and arg in s else 0)
                if score > score_best:
                    best, score_best = (path, i), score
    return best


# ── editing ───────────────────────────────────────────────────────────────────
def _backup(path: str) -> str:
    os.makedirs(BACKUP_DIR, exist_ok=True)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    dst = os.path.join(BACKUP_DIR, f"{os.path.basename(path)}.{stamp}")
    n = 2
    while os.path.exists(dst):
        dst = os.path.join(BACKUP_DIR, f"{os.path.basename(path)}.{stamp}-{n}")
        n += 1
    shutil.copy2(path, dst)
    return dst


def _config_errors() -> list[str]:
    errs = hypr.query("configerrors") or []
    return [e for e in errs if isinstance(e, str) and e.strip()]


def _write_and_reload(path: str, new_text: str) -> str:
    """Write `path`, reload Hyprland, and put the old file back if Hyprland complains. Returns '' or an error."""
    with open(path, "r", encoding="utf-8") as f:
        old = f.read()
    backup = _backup(path)
    tmp = path + ".luminescent-tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(new_text)
    os.replace(tmp, path)
    hypr.request("reload")
    time.sleep(0.4)
    errs = _config_errors()
    if errs:
        with open(path, "w", encoding="utf-8") as f:
            f.write(old)
        hypr.request("reload")
        return "Hyprland did not accept the change, so it was undone:\n" + "\n".join(errs[:5])
    log.info("edited %s (backup %s)", path, backup)
    return ""


def _fresh(call: Call) -> Call | None:
    """The same call in the file as it is now (it may have changed since it was listed)."""
    try:
        with open(call.file, "r", encoding="utf-8") as f:
            now = f.read()
    except OSError:
        return None
    if now != call.src:
        return None
    return call


def to_hypr_keys(mods: list[str], key: str) -> str:
    return " + ".join(mods + [key])


def change_keys(bind: Bind, new_keys: str) -> str:
    """Give an hl.bind(...) other keys ('SUPER + SHIFT + G'). Returns '' or an error."""
    call = bind.call
    if call is None:
        return "This bind was not found in your config files."
    if call.in_loop:
        return "This bind is made in a loop; edit that loop in the file instead."
    if _fresh(call) is None:
        return "The file changed since the list was loaded. Refresh and try again."
    start, end = call.arg_span(0)
    old_expr = call.src[start:end]
    # keep `mainMod .. " + G"` style: reuse the leading variable if it still fits
    m = re.fullmatch(r"\s*([A-Za-z_]\w*)\s*\.\.\s*([\"'])(.*)\2\s*", old_expr, re.S)
    new_expr = '"' + new_keys.replace('"', '\\"') + '"'
    if m:
        var_val = call.local_vars.get(m.group(1))
        if isinstance(var_val, str) and new_keys.startswith(var_val + " "):
            rest = new_keys[len(var_val):]
            new_expr = f'{m.group(1)} .. "{rest}"'
    return _write_and_reload(call.file, call.src[:start] + new_expr + call.src[end:])


def delete(bind: Bind) -> str:
    """Comment out an hl.bind(...). Returns '' or an error."""
    call = bind.call
    if call is None:
        return "This bind was not found in your config files."
    if call.in_loop:
        return "This bind is made in a loop; edit that loop in the file instead."
    if _fresh(call) is None:
        return "The file changed since the list was loaded. Refresh and try again."
    src = call.src
    line_start = src.rfind("\n", 0, call.start) + 1
    line_end = src.find("\n", call.end)
    line_end = len(src) if line_end < 0 else line_end
    if src[line_start:call.start].strip() or src[call.end:line_end].strip():
        return "That line holds more than this bind; edit it in the file instead."
    block = src[line_start:line_end]
    commented = "\n".join("-- " + ln for ln in block.split("\n"))
    return _write_and_reload(call.file, src[:line_start] + commented + src[line_end:])


def binds_file() -> str:
    """Where new binds go: the config file with the most hl.bind calls."""
    best, n_best = os.path.join(CONFIG_DIR, "hyprland.lua"), -1
    for pf in parse_config():
        if len(pf.calls) > n_best:
            best, n_best = pf.path, len(pf.calls)
    return best


def add(keys: str, command: str) -> str:
    """Add hl.bind(keys, hl.dsp.exec_cmd(command)) to your config. Returns '' or an error."""
    path = binds_file()
    if not os.path.isfile(path):
        return f"{path} does not exist (is your config hyprland.lua?)"
    with open(path, "r", encoding="utf-8") as f:
        src = f.read()
    q = lambda s: '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'
    line = f"hl.bind({q(keys)}, hl.dsp.exec_cmd({q(command)}))"
    if ADDED_MARK not in src:
        src = src.rstrip("\n") + f"\n\n{ADDED_MARK}\n"
    return _write_and_reload(path, src.rstrip("\n") + "\n" + line + "\n")


def open_in_editor(path: str, line: int) -> str:
    """Open `path` at `line` in a code editor if one is installed, else with the default app."""
    candidates = [
        ("code", ["code", "-g", f"{path}:{line}"]), ("codium", ["codium", "-g", f"{path}:{line}"]),
        ("zed", ["zed", f"{path}:{line}"]), ("kate", ["kate", "-l", str(line), path]),
        ("kwrite", ["kwrite", "-l", str(line), path]), ("gedit", ["gedit", f"+{line}", path]),
        ("gnome-text-editor", ["gnome-text-editor", f"+{line}", path]),
    ]
    from core import runner, terminal
    for exe, cmd in candidates:
        if shutil.which(exe):
            try:
                runner._spawn(cmd)
                return ""
            except OSError:
                continue
    editor = os.environ.get("VISUAL") or os.environ.get("EDITOR")
    if editor and shutil.which(editor.split()[0]):
        full = terminal.command("Edit", os.path.dirname(path), editor.split() + [f"+{line}", path])
        if full:
            runner._spawn(full)
            return ""
    return runner.open_path(path)
