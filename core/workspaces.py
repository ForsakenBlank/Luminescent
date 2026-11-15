"""
Workspace sets for Hyprland (System Settings → Workspaces).

Workspaces are split into sets of the same size: with 4 per set, set 1 is 1-4, set 2 is 5-8 and so on.
With more than one monitor every monitor has its own sets ("split"): with 2 sets of 4 the first monitor has 1-8 and
the second 9-16, so the keys never throw you onto the other screen. The first monitor is the one your own config
gives workspace 1 to, else the leftmost. Switched off, all monitors share 1-8 as before.

  * Ctrl+Super+Left / Right go through the workspaces of the current set, then the monitor's named workspaces
    ("gaming"), then round to the other end (or stop, or carry on into the next set).
  * Ctrl+Super+Up / Down go to the same spot in the set above / below (2 → 6 → 10).
  * Every switch slides the way you pressed. Hyprland alone slides by workspace number, so going round from 1 to
    4 with Left came in from the wrong side; see the note above _LUA.
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

DEFAULTS = {"on": False, "per": 4, "sets": 2, "wrap": "round", "anim": "slide", "named": True, "move_keys": True,
            "scroll": False, "scroll_invert": False, "persistent": True, "bar": True, "split": True, "first": "",
            "digits": False}

WRAPS = [("round", "Goes round to the other end of the set"),
         ("next_set", "Carries on into the next set (Right on 4 goes to 5)"),
         ("stop", "Stops")]
ANIMS = [("slide", "Slide the way you pressed"), ("fade", "Fade"), ("none", "No animation"),
         ("hypr", "Leave it to Hyprland (can slide the wrong way)")]

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
    if out["wrap"] not in dict(WRAPS):                # 1.9.0's fade / vertical / instant / slide all went round
        out["wrap"] = "round"
    if out["anim"] not in dict(ANIMS):
        out["anim"] = "slide"
    return out


def available() -> bool:
    """Hyprland with the Lua API (0.55+)."""
    return hypr.available() and hypr.value("type(hl.timer)") == "function"


def _anim() -> dict:
    """How Hyprland animates workspaces now, so it can be put back after a key press."""
    for a in hypr.query("animations") or []:
        items = a if isinstance(a, list) else [a]
        for x in items:
            if isinstance(x, dict) and x.get("name") == "workspaces":
                return {"enabled": bool(x.get("enabled", True)), "speed": float(x.get("speed") or 5),
                        "bezier": x.get("bezier") or "default", "style": x.get("style") or "slide"}
    return {"enabled": True, "speed": 5.0, "bezier": "default", "style": "slide"}


def _slide_parts(style: str) -> tuple[str, str]:
    """Your own workspace animation as (kind, how far): 'slidefade 20%' stays a slidefade of 20 %, anything else
    is a plain slide. The direction goes in between: 'slide left', 'slidefade top 20%'."""
    words = (style or "").split()
    kind = "slidefade" if words and words[0].startswith("slidefade") else "slide"
    perc = " " + words[-1] if words and words[-1].endswith("%") else ""
    return kind, perc


# Hyprland picks the direction of a workspace slide from the workspace numbers alone (a higher number comes in
# from the right), so going round from 1 to 4 with Left slid the wrong way, and so did anything to or from a named
# workspace. Its animation style takes the direction as a second word though ("slide left", "slide top"): every
# key press sets the style for the way that was pressed, switches, and puts your own style back.
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
W.per, W.sets, W.wrap, W.mode, W.split, W.extras = __PER__, __SETS__, __WRAP__, __MODE__, __SPLIT__, __NAMED__
W.kind, W.perc = __KIND__, __PERC__
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
-- where you are in this monitor's block (1 .. per * sets), the block's base, whether the workspace on screen
-- really is one of the block's, and its name when it is a named one ("gaming")
function W.cur()
  local base, name, m = W.base()
  local total = W.per * W.sets
  local w = m and m.active_workspace or hl.get_active_workspace()
  local id = w and not w.special and w.id
  if id and id > base and id <= base + total then W.last[name] = id return id - base, base, true end
  local named = w and not w.special and not w.id and w.name or nil
  local l = W.last[name]
  if l and l > base and l <= base + total then return l - base, base, false, named end
  return 1, base, false, named
end
-- the named workspaces on the focused monitor, in name order: they sit after the last workspace of a set, so
-- the arrow keys reach them too
function W.named()
  local out = {}
  if not W.extras then return out end
  local m = hl.get_active_monitor()
  for _, w in ipairs(hl.get_workspaces()) do
    if not w.special and not w.id and w.name and w.monitor and m and w.monitor.name == m.name then
      out[#out + 1] = w.name
    end
  end
  table.sort(out)
  return out
end
-- dir: "left" / "right" / "top" / "bottom", the way the new workspace should come in from
function W.style(dir)
  if W.mode == "hypr" or not dir then return nil end
  if W.mode == "fade" then return "fade" end
  if W.mode == "none" then return "none" end
  return W.kind .. " " .. dir .. W.perc
end
function W.go(target, dir, move)
  local m = hl.get_active_monitor()
  local cur = m and m.active_workspace
  local num = type(target) == "number"
  if not move and cur then
    if num and cur.id == target then return end
    if not num and not cur.id and "name:" .. tostring(cur.name) == target then return end
  end
  if num and W.split and m then                -- one of this monitor's workspaces sits on another screen: bring it back
    local ok, t = pcall(hl.get_workspace, target)
    if ok and t and t.monitor and t.monitor.name ~= m.name then
      pcall(function() hl.dispatch(hl.dsp.workspace.move({ workspace = target, monitor = m.name })) end)
    end
  end
  local d
  if move then d = hl.dsp.window.move({ workspace = target }) else d = hl.dsp.focus({ workspace = target }) end
  local want = W.style(dir) or W.anim.style
  if W.timer then pcall(function() W.timer:set_enabled(false) end) W.timer = nil end
  if want ~= W.anim.style then
    local a = {}
    for k, v in pairs(W.anim) do a[k] = v end
    if want == "none" then a.enabled = false else a.style = want a.enabled = true end
    pcall(hl.animation, a)
    W.temp = true
  elseif W.temp then
    pcall(hl.animation, W.anim)
    W.temp = false
  end
  hl.dispatch(d)
  if num then W.last[W.split and m and m.name or ""] = target end
  if W.temp then                               -- your own animation again once this one has played
    W.timer = hl.timer(function() pcall(hl.animation, W.anim) W.temp = false W.timer = nil end,
                       { timeout = W.ms, type = "oneshot" })
  end
end
-- Left / Right: along the set (or all sets with "next_set"), then the monitor's named workspaces, then round
function W.step(dx, move)
  local n, base, here, named = W.cur()
  local dir = dx > 0 and "right" or "left"
  local names = W.named()
  if not here and not named then return W.go(base + n, dir, move) end   -- a workspace of another block: back home
  local per, total = W.per, W.per * W.sets
  local lo, hi = 1, total
  if W.wrap ~= "next_set" then
    lo = math.floor((n - 1) / per) * per + 1
    hi = lo + per - 1
  end
  local ring, at = {}, nil
  for i = lo, hi do
    ring[#ring + 1] = base + i
    if here and i == n then at = #ring end
  end
  for _, nm in ipairs(names) do
    ring[#ring + 1] = "name:" .. nm
    if named == nm then at = #ring end
  end
  if not at then return W.go(base + n, dir, move) end
  local to = at + dx
  if to < 1 or to > #ring then
    if W.wrap == "stop" then return end
    to = (to - 1) % #ring + 1
  end
  W.go(ring[to], dir, move)
end
-- Up / Down: the same spot in the set above / below, always sliding the way you pressed
function W.set(dy, move)
  if W.sets < 2 then return end
  local n, base = W.cur()
  local set, pos = math.floor((n - 1) / W.per), (n - 1) % W.per
  local ns = set + dy
  if ns < 0 or ns >= W.sets then
    if W.wrap == "stop" then return end
    ns = ns % W.sets
  end
  W.go(base + ns * W.per + pos + 1, dy > 0 and "bottom" or "top", move)
end
function W.pick(i, move)                       -- the i-th workspace of the set you are in
  if i > W.per then return end
  local n, base, here = W.cur()
  local pos = (n - 1) % W.per + 1
  W.go(base + math.floor((n - 1) / W.per) * W.per + i, (i < pos and here) and "left" or "right", move)
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


def _lua_code(cfg: dict, anim: dict, order: list[str], own_rules: set, stray: list[str]) -> str:
    q = hypr.lstr
    split = cfg["split"] and len(order) > 1
    kind, perc = _slide_parts(anim["style"])
    code = (_LUA.replace("__PER__", str(cfg["per"])).replace("__SETS__", str(cfg["sets"]))
            .replace("__WRAP__", q(cfg["wrap"])).replace("__MODE__", q(cfg["anim"]))
            .replace("__NAMED__", "true" if cfg["named"] else "false")
            .replace("__KIND__", q(kind)).replace("__PERC__", q(perc))
            .replace("__SPLIT__", "true" if split else "false")
            .replace("__ORDER__", ", ".join(q(n) for n in order))
            .replace("__AEN__", "true" if anim["enabled"] else "false").replace("__ASPEED__", repr(anim["speed"]))
            .replace("__ABEZ__", q(anim["bezier"])).replace("__ASTYLE__", q(anim["style"]))
            .replace("__DESC__", q(hypr.BIND_DESC)))
    lines = [code] + list(stray)                      # before the rules: a rule would pull the workspace over
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
    return "\n".join(lines)


def _stray(cfg: dict, order: list[str], workspaces: list | None, clients: list | None = None) -> list[str]:
    """Lua for open workspaces whose number belongs to another monitor's block (workspace 4 open on the second
    monitor, which now has 9-16). They stay on the screen they are on and get the number of the same spot in
    that monitor's block (4 becomes 12), so no window jumps to another monitor when sets are switched on. If
    that number is taken, the windows move over to it instead."""
    if not cfg["split"] or len(order) < 2:
        return []
    q = hypr.lstr
    block = cfg["per"] * cfg["sets"]
    taken = {w.get("id") for w in workspaces or [] if isinstance(w, dict)}
    out = []
    for w in workspaces or []:
        wid, mon = (w.get("id"), w.get("monitor")) if isinstance(w, dict) else (None, None)
        if not isinstance(wid, int) or wid < 1 or wid > block * len(order) or mon not in order:
            continue
        if order[(wid - 1) // block] == mon:
            continue
        new = order.index(mon) * block + (wid - 1) % block + 1
        if new not in taken:
            taken.discard(wid)
            taken.add(new)
            out.append(f"pcall(function() hl.dispatch(hl.dsp.workspace.change_id({{ workspace = {wid}, "
                       f"id = {new} }})) end)")
            continue
        for c in clients or []:
            if isinstance(c, dict) and (c.get("workspace") or {}).get("id") == wid and c.get("address"):
                out.append(f"pcall(function() hl.dispatch(hl.dsp.window.move({{ workspace = {new}, "
                           f"window = {q('address:' + c['address'])}, follow = false }})) end)")
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
    stray = _stray(cfg, order, hypr.query("workspaces"), hypr.query("clients"))
    err = hypr.lua(_lua_code(cfg, _anim(), order, own, stray))
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
