"""
Workspace sets for Hyprland (System Settings → Workspaces).

Workspaces are split into sets of the same size: with 4 per set, set 1 is 1-4, set 2 is 5-8 and so on.
With more than one monitor every monitor has its own sets ("split"): with 2 sets of 4 the first monitor has 1-8 and
the second 9-16, so the keys never throw you onto the other screen. The first monitor is the one your own config
gives workspace 1 to, else the leftmost. Switched off, all monitors share 1-8 as before.

  * Ctrl+Super+Left / Right go through the workspaces of the current set. Hyprland slides by workspace number, so
    going round from 4 back to 1 used to slide *left* although you pressed right. How the wrap looks is now a
    choice: a fade, a vertical slide, no animation, stop at the ends, carry on into the next set, or Hyprland's
    own slide.
  * Ctrl+Super+Up / Down go to the same spot in the previous / next set (2 → 6 → 10), sliding vertically.
  * Optionally the same keys with Shift move the active window along, Super + the scroll wheel cycles too, and
    Ctrl+Super+1…9 go straight to that workspace of the set you are in.
  * More workspaces: every workspace of every set can be kept (persistent) so it is there before anything opens on
    it, and Caelestia's bar is told how many make a set, so it shows the set you are in.

All of it is Lua inside Hyprland (Hyprland 0.55+ with a Lua config): the keys answer instantly and keep working
whatever Luminescent is doing. Hyprland forgets it when its config reloads; the System Settings module puts it back.
Switching it off reloads Hyprland's config, which brings your own binds for those keys back.
"""
import json
import logging

from core import hypr

log = logging.getLogger("workspaces")

DEFAULTS = {"on": False, "per": 4, "sets": 2, "wrap": "fade", "vertical": "slidevert", "move_keys": True,
            "scroll": False, "scroll_invert": False, "persistent": True, "bar": True, "split": True, "first": "",
            "digits": False}

WRAPS = [("fade", "Fade (no direction)"), ("vertical", "Slide up / down"), ("instant", "No animation"),
         ("next_set", "Carry on into the next set (Right on 4 goes to 5)"), ("stop", "Stop at the first and last"),
         ("slide", "Slide like Hyprland does (backwards when it wraps)")]
VERTICALS = [("slidevert", "Slide up / down"), ("fade", "Fade"), ("slide", "Slide sideways"),
             ("none", "No animation")]

MODS = ["SUPER + CONTROL", "CONTROL + SUPER", "SUPER + CTRL", "CTRL + SUPER"]
MODS_SHIFT = ["SUPER + CONTROL + SHIFT", "SUPER + SHIFT + CONTROL", "CONTROL + SHIFT + SUPER", "SHIFT + SUPER + CONTROL",
              "CONTROL + SUPER + SHIFT", "SHIFT + CONTROL + SUPER", "SUPER + CTRL + SHIFT", "CTRL + SUPER + SHIFT"]


def config(saved: dict | None) -> dict:
    out = dict(DEFAULTS)
    if isinstance(saved, dict):
        out.update({k: v for k, v in saved.items() if k in DEFAULTS})
    out["per"] = max(1, min(10, int(out["per"])))
    out["sets"] = max(1, min(10, int(out["sets"])))
    out["first"] = str(out["first"] or "")
    return out


def available() -> bool:
    """Hyprland with the Lua API (0.55+)."""
    return hypr.available() and hypr.value("type(hl.timer)") == "function"


def _anim() -> dict:
    """How Hyprland animates workspaces now, so it can be put back after a one-off fade."""
    for a in hypr.query("animations") or []:
        items = a if isinstance(a, list) else [a]
        for x in items:
            if isinstance(x, dict) and x.get("name") == "workspaces":
                return {"enabled": bool(x.get("enabled", True)), "speed": float(x.get("speed") or 5),
                        "bezier": x.get("bezier") or "default", "style": x.get("style") or "slide"}
    return {"enabled": True, "speed": 5.0, "bezier": "default", "style": "slide"}


_LUA = r"""
LUMI = LUMI or {}
LUMI.ws = LUMI.ws or {}
local W = LUMI.ws
for _, b in pairs(W.binds or {}) do pcall(function() b:remove() end) end
for _, r in pairs(W.rules or {}) do pcall(function() r:set_enabled(false) end) end
W.binds, W.rules = {}, {}
-- a key gets one bind that stays until Hyprland's config reloads; changing a setting only switches binds on and
-- off and swaps what they call (removing binds over and over can crash Hyprland 0.56, see core/hypr.py)
W.slots = W.slots or {}
W.fns = {}
for _, s in pairs(W.slots) do pcall(function() s:set_enabled(false) end) end
W.per, W.sets, W.wrap, W.vert, W.split = __PER__, __SETS__, __WRAP__, __VERT__, __SPLIT__
W.order = { __ORDER__ }
if type(W.last) ~= "table" then W.last = {} end
W.anim = { leaf = "workspaces", enabled = __AEN__, speed = __ASPEED__, bezier = __ABEZ__, style = __ASTYLE__ }
W.ms = math.floor(__ASPEED__ * 100 + 200)
W.temp = false
-- the focused monitor's name and the number its workspaces start after: with a block of 8, the first monitor has
-- 1-8 (base 0) and the second 9-16 (base 8). One shared pool (W.split off) always starts at 0.
function W.base()
  local m = hl.get_active_monitor()
  local name = m and m.name or ""
  if not W.split then return 0, "", m end
  for i, n in ipairs(W.order) do
    if n == name then return (i - 1) * W.per * W.sets, name, m end
  end
  W.order[#W.order + 1] = name                 -- a monitor plugged in later gets the next block
  return (#W.order - 1) * W.per * W.sets, name, m
end
-- where you are in this monitor's block (1 .. per * sets), the block's base, and whether the workspace on screen
-- really is one of the block's (false on a named workspace like "gaming" or one that belongs to another monitor)
function W.cur()
  local base, name, m = W.base()
  local total = W.per * W.sets
  local w = m and m.active_workspace or hl.get_active_workspace()
  local id = w and not w.special and w.id
  if id and id > base and id <= base + total then W.last[name] = id return id - base, base, true end
  local l = W.last[name]
  if l and l > base and l <= base + total then return l - base, base, false end
  return 1, base, false
end
function W.go(target, style, move)
  local m = hl.get_active_monitor()
  local cur = m and m.active_workspace
  if not move and cur and cur.id == target then return end
  if W.split and m then                        -- one of this monitor's workspaces sits on another screen: bring it back
    local ok, t = pcall(hl.get_workspace, target)
    if ok and t and t.monitor and t.monitor.name ~= m.name then
      pcall(function() hl.dispatch(hl.dsp.workspace.move({ workspace = target, monitor = m.name })) end)
    end
  end
  local d
  if move then d = hl.dsp.window.move({ workspace = target }) else d = hl.dsp.focus({ workspace = target }) end
  local want = style or W.anim.style
  if W.timer then pcall(function() W.timer:set_enabled(false) end) W.timer = nil end
  if want ~= W.anim.style then
    local a = {}
    for k, v in pairs(W.anim) do a[k] = v end
    if want == "none" then a.enabled = false else a.style = want a.enabled = true end
    pcall(hl.animation, a)
    W.temp = true
  elseif W.temp then                           -- a fade from a moment ago is still set: your own animation first
    pcall(hl.animation, W.anim)
    W.temp = false
  end
  hl.dispatch(d)
  W.last[W.split and m and m.name or ""] = target
  if W.temp then
    W.timer = hl.timer(function() pcall(hl.animation, W.anim) W.temp = false W.timer = nil end,
                       { timeout = W.ms, type = "oneshot" })
  end
end
function W.step(dx, move)
  local n, base, here = W.cur()
  if not here then return W.go(base + n, nil, move) end      -- from "gaming" and the like: back to where you were
  local per, total = W.per, W.per * W.sets
  local set, pos = math.floor((n - 1) / per), (n - 1) % per
  local np = pos + dx
  if np >= 0 and np < per then return W.go(base + set * per + np + 1, nil, move) end
  if W.wrap == "stop" then return end
  if W.wrap == "next_set" then
    local t = n + dx
    if t >= 1 and t <= total then return W.go(base + t, nil, move) end
    return W.go(base + ((t - 1) % total) + 1, "fade", move)
  end
  local styles = { fade = "fade", vertical = "slidevert", instant = "none" }
  W.go(base + set * per + (np % per) + 1, styles[W.wrap], move)
end
function W.set(dy, move)
  if W.sets < 2 then return end
  local n, base, here = W.cur()
  if not here then return W.go(base + n, nil, move) end
  local set, pos = math.floor((n - 1) / W.per), (n - 1) % W.per
  local ns = set + dy
  local style = W.vert
  if ns < 0 or ns >= W.sets then               -- round from the last set to the first: the slide would go the wrong way
    ns = ns % W.sets
    if style == "slidevert" or style == "slide" then style = "fade" end
  end
  W.go(base + ns * W.per + pos + 1, style, move)
end
function W.pick(i, move)                       -- the i-th workspace of the set you are in
  if i > W.per then return end
  local n, base = W.cur()
  W.go(base + math.floor((n - 1) / W.per) * W.per + i, nil, move)
end
function W.key(keys, others, fn)
  W.fns[keys] = fn
  local s = W.slots[keys]
  if s then pcall(function() s:set_enabled(true) end) return end
  for _, k in ipairs(others) do pcall(hl.unbind, k) end      -- your own binds for the key, however they are spelt
  W.slots[keys] = hl.bind(keys, function() local f = W.fns[keys] if f then f() end end, { description = __DESC__ })
end
"""


def monitor_order(mons: list | None = None, rules: list | None = None, first: str = "") -> list[str]:
    """The monitors in the order their blocks of workspaces go. A monitor your own config already gives
    workspaces to (workspace rule "1" on HDMI-A-2) keeps them, so it comes first; the rest go left to right.
    `first` (Workspaces → First monitor) puts that one in front whatever the config says."""
    mons = hypr.query("monitors") if mons is None else mons
    rules = hypr.query("workspacerules") if rules is None else rules
    lowest: dict[str, int] = {}
    for r in rules or []:
        if not isinstance(r, dict):
            continue
        ws, mon = str(r.get("workspaceString", "")), r.get("monitor")
        if ws.isdigit() and mon:
            lowest[mon] = min(lowest.get(mon, 10 ** 9), int(ws))
    real = [m for m in mons or [] if isinstance(m, dict) and m.get("name")]
    real.sort(key=lambda m: (lowest.get(m["name"], 10 ** 9), m.get("x", 0), m.get("y", 0), m["name"]))
    names = [m["name"] for m in real]
    if first in names:
        names.remove(first)
        names.insert(0, first)
    return names


def layout(cfg: dict, order: list[str]) -> list[tuple[str, list[list[int]]]]:
    """[(monitor name or '', [[the workspaces of set 1], [of set 2]...])]: what the settings page draws."""
    per, sets = cfg["per"], cfg["sets"]
    names = order if cfg["split"] and len(order) > 1 else [""]
    return [(name, [[k * per * sets + s * per + i + 1 for i in range(per)] for s in range(sets)])
            for k, name in enumerate(names)]


def _lua_code(cfg: dict, anim: dict, order: list[str], own_rules: set, stray: list[tuple[int, str]]) -> str:
    q = hypr.lstr
    split = cfg["split"] and len(order) > 1
    code = (_LUA.replace("__PER__", str(cfg["per"])).replace("__SETS__", str(cfg["sets"]))
            .replace("__WRAP__", q(cfg["wrap"])).replace("__VERT__", q(cfg["vertical"]))
            .replace("__SPLIT__", "true" if split else "false")
            .replace("__ORDER__", ", ".join(q(n) for n in order))
            .replace("__AEN__", "true" if anim["enabled"] else "false").replace("__ASPEED__", repr(anim["speed"]))
            .replace("__ABEZ__", q(anim["bezier"])).replace("__ASTYLE__", q(anim["style"]))
            .replace("__DESC__", q(hypr.BIND_DESC)))
    lines = [code]
    keys = [("Right", "W.step(1)"), ("Left", "W.step(-1)")]
    if cfg["sets"] > 1:
        keys += [("Down", "W.set(1)"), ("Up", "W.set(-1)")]
    def others(mods: list[str], key: str) -> str:
        return "{ " + ", ".join(q(m + " + " + key) for m in mods) + " }"

    for key, call in keys:
        lines.append(f"W.key({q('SUPER + CONTROL + ' + key)}, {others(MODS, key)}, function() {call} end)")
        if cfg["move_keys"]:
            lines.append(f"W.key({q('SUPER + CONTROL + SHIFT + ' + key)}, {others(MODS_SHIFT, key)}, "
                         f"function() {call[:-1]}, true) end)")
    if cfg["digits"]:                                 # by key code (10 = the 1 key), so it works on every layout
        for i in range(1, min(cfg["per"], 10) + 1):
            code, sym = f"code:{9 + i}", str(i % 10)
            lines.append(f"W.key({q('SUPER + CONTROL + ' + code)}, "
                         f"{others(MODS, code)[:-2]}, {others(MODS, sym)[2:]}, function() W.pick({i}) end)")
            if cfg["move_keys"]:
                lines.append(f"W.key({q('SUPER + CONTROL + SHIFT + ' + code)}, {others(MODS_SHIFT, code)[:-2]}, "
                             f"{others(MODS_SHIFT, sym)[2:]}, function() W.pick({i}, true) end)")
    if cfg["scroll"]:
        up, down = ("W.step(-1)", "W.step(1)") if cfg["scroll_invert"] else ("W.step(1)", "W.step(-1)")
        for key, call in (("mouse_up", up), ("mouse_down", down)):
            lines.append(f"W.key({q('SUPER + ' + key)}, {others(['SUPER'], key)}, function() {call} end)")
    block = cfg["per"] * cfg["sets"]
    names = order if split else order[:1]
    if cfg["persistent"]:
        for k, name in enumerate(names or [""]):
            for i in range(k * block + 1, (k + 1) * block + 1):
                if str(i) in own_rules:
                    continue                          # your own config already has a rule for it
                mon = f", monitor = {q(name)}" if name else ""
                lines.append(f"W.rules[#W.rules + 1] = hl.workspace_rule({{ workspace = {q(str(i))}{mon}, "
                             f"persistent = true }})")
    for wid, name in stray:                           # a workspace that is on the wrong screen for its block
        lines.append(f"pcall(function() hl.dispatch(hl.dsp.workspace.move({{ workspace = {wid}, "
                     f"monitor = {q(name)} }})) end)")
    return "\n".join(lines)


def _stray(cfg: dict, order: list[str], workspaces: list | None) -> list[tuple[int, str]]:
    """Open workspaces that sit on another monitor than the one their number belongs to."""
    if not cfg["split"] or len(order) < 2:
        return []
    block = cfg["per"] * cfg["sets"]
    out = []
    for w in workspaces or []:
        wid = w.get("id") if isinstance(w, dict) else None
        if not isinstance(wid, int) or wid < 1 or wid > block * len(order):
            continue
        owner = order[(wid - 1) // block]
        if w.get("monitor") != owner:
            out.append((wid, owner))
    return out


def apply(saved: dict | None) -> str:
    """Install the binds (or do nothing when switched off). '' or what went wrong."""
    cfg = config(saved)
    if not cfg["on"]:
        return ""
    if not available():
        return "needs Hyprland 0.55 or newer with a Lua config"
    # a one-off fade may be set this very moment: put your own animation back before reading it
    hypr.lua("if LUMI and LUMI.ws and LUMI.ws.temp then pcall(hl.animation, LUMI.ws.anim) LUMI.ws.temp = false end")
    rules = [r for r in (hypr.query("workspacerules") or []) if isinstance(r, dict)]
    order = monitor_order(None, rules, str(cfg["first"] or ""))
    own = {r.get("workspaceString") for r in rules}
    err = hypr.lua(_lua_code(cfg, _anim(), order, own, _stray(cfg, order, hypr.query("workspaces"))))
    if not err and cfg["bar"]:
        err = set_caelestia_shown(cfg["per"])
    return err


def turn_off() -> str:
    """Remove ours and reload Hyprland's config so your own binds for those keys come back."""
    if not hypr.available():
        return ""
    hypr.lua("if LUMI and LUMI.ws then local W = LUMI.ws "
             "if W.temp then pcall(hl.animation, W.anim) W.temp = false end "
             "for _, b in pairs(W.binds or {}) do pcall(function() b:remove() end) end "
             "for _, s in pairs(W.slots or {}) do pcall(function() s:set_enabled(false) end) end "
             "for _, r in pairs(W.rules or {}) do pcall(function() r:set_enabled(false) end) end "
             "W.binds, W.rules = {}, {} end")
    hypr.request("reload")
    return ""


def set_caelestia_shown(n: int) -> str:
    """Caelestia's bar shows workspaces in groups of bar.workspaces.shown: one group = one set."""
    from core import syscontrol as sc
    import os
    path = sc._CAEL_SHELL
    if not os.path.isfile(path):
        return ""                                   # no Caelestia: nothing to tell
    d = sc._cael_read()
    ws = d.setdefault("bar", {}).setdefault("workspaces", {})
    if ws.get("shown") == n:
        return ""
    ws["shown"] = n
    try:
        tmp = path + ".lum-tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(d, f, indent=4)
            f.write("\n")
        os.replace(tmp, path)
    except OSError as e:
        return f"Could not write Caelestia's shell.json: {e}"
    return ""
