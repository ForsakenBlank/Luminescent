"""
Workspace sets for Hyprland (System Settings → Workspaces).

Workspaces are split into sets of the same size: with 4 per set, set 1 is 1-4, set 2 is 5-8 and so on.

  * Ctrl+Super+Left / Right go through the workspaces of the current set. Hyprland slides by workspace number, so
    going round from 4 back to 1 used to slide *left* although you pressed right. How the wrap looks is now a
    choice: a fade, a vertical slide, no animation, stop at the ends, carry on into the next set, or Hyprland's
    own slide.
  * Ctrl+Super+Up / Down go to the same spot in the previous / next set (2 → 6 → 10), sliding vertically.
  * Optionally the same keys with Shift move the active window along, and Super + the scroll wheel cycles too.
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
            "scroll": False, "scroll_invert": False, "persistent": True, "bar": True}

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
W.per, W.sets, W.wrap, W.vert = __PER__, __SETS__, __WRAP__, __VERT__
W.anim = { leaf = "workspaces", enabled = __AEN__, speed = __ASPEED__, bezier = __ABEZ__, style = __ASTYLE__ }
W.ms = math.floor(__ASPEED__ * 100 + 200)
function W.cur()
  local w = hl.get_active_workspace()
  if w and w.id and w.id >= 1 and not w.special then W.last = w.id return w.id end
  return W.last or 1
end
function W.go(target, style, move)
  local d
  if move then d = hl.dsp.window.move({ workspace = target }) else d = hl.dsp.focus({ workspace = target }) end
  if style and style ~= W.anim.style then
    local a = {}
    for k, v in pairs(W.anim) do a[k] = v end
    if style == "none" then a.enabled = false else a.style = style a.enabled = true end
    pcall(hl.animation, a)
    hl.dispatch(d)
    if W.timer then pcall(function() W.timer:set_enabled(false) end) end
    W.timer = hl.timer(function() pcall(hl.animation, W.anim) end, { timeout = W.ms, type = "oneshot" })
  else
    hl.dispatch(d)
  end
end
function W.step(dx, move)
  local id = W.cur()
  local per, total = W.per, W.per * W.sets
  local set, pos = math.floor((id - 1) / per), (id - 1) % per
  local np = pos + dx
  if np >= 0 and np < per then return W.go(set * per + np + 1, nil, move) end
  if W.wrap == "stop" then return end
  if W.wrap == "next_set" then
    local t = id + dx
    if t >= 1 and t <= total then return W.go(t, nil, move) end
    return W.go(((t - 1) % total) + 1, "fade", move)
  end
  local styles = { fade = "fade", vertical = "slidevert", instant = "none" }
  W.go(set * per + (np % per) + 1, styles[W.wrap], move)
end
function W.set(dy, move)
  if W.sets < 2 then return end
  local id = W.cur()
  local set, pos = math.floor((id - 1) / W.per), (id - 1) % W.per
  local ns = (set + dy) % W.sets
  W.go(ns * W.per + pos + 1, W.vert, move)
end
function W.key(keys, fn)
  W.binds[#W.binds + 1] = hl.bind(keys, fn, { description = __DESC__ })
end
"""


def _lua_code(cfg: dict, anim: dict, monitor: str, have_rules: set) -> str:
    q = hypr.lstr
    code = (_LUA.replace("__PER__", str(cfg["per"])).replace("__SETS__", str(cfg["sets"]))
            .replace("__WRAP__", q(cfg["wrap"])).replace("__VERT__", q(cfg["vertical"]))
            .replace("__AEN__", "true" if anim["enabled"] else "false").replace("__ASPEED__", repr(anim["speed"]))
            .replace("__ABEZ__", q(anim["bezier"])).replace("__ASTYLE__", q(anim["style"]))
            .replace("__DESC__", q(hypr.BIND_DESC)))
    lines = [code]
    keys = [("Right", "W.step(1)"), ("Left", "W.step(-1)")]
    if cfg["sets"] > 1:
        keys += [("Down", "W.set(1)"), ("Up", "W.set(-1)")]
    for key, call in keys:
        for m in MODS:
            lines.append(f"pcall(hl.unbind, {q(m + ' + ' + key)})")
        lines.append(f"W.key({q('SUPER + CONTROL + ' + key)}, function() {call} end)")
        if cfg["move_keys"]:
            for m in MODS_SHIFT:
                lines.append(f"pcall(hl.unbind, {q(m + ' + ' + key)})")
            lines.append(f"W.key({q('SUPER + CONTROL + SHIFT + ' + key)}, function() {call[:-1]}, true) end)")
    if cfg["scroll"]:
        up, down = ("W.step(-1)", "W.step(1)") if cfg["scroll_invert"] else ("W.step(1)", "W.step(-1)")
        for key, call in (("mouse_up", up), ("mouse_down", down)):
            lines.append(f"pcall(hl.unbind, {q('SUPER + ' + key)})")
            lines.append(f"W.key({q('SUPER + ' + key)}, function() {call} end)")
    if cfg["persistent"]:
        for i in range(1, cfg["per"] * cfg["sets"] + 1):
            if str(i) in have_rules:
                continue                          # your own config already has a rule for it
            mon = f", monitor = {q(monitor)}" if monitor else ""
            lines.append(f"W.rules[#W.rules + 1] = hl.workspace_rule({{ workspace = {q(str(i))}{mon}, "
                         f"persistent = true }})")
    return "\n".join(lines)


def apply(saved: dict | None) -> str:
    """Install the binds (or do nothing when switched off). '' or what went wrong."""
    cfg = config(saved)
    if not cfg["on"]:
        return ""
    if not available():
        return "needs Hyprland 0.55 or newer with a Lua config"
    mons = hypr.query("monitors") or []
    focused = next((m for m in mons if m.get("focused")), mons[0] if mons else {})
    own = {r.get("workspaceString") for r in (hypr.query("workspacerules") or []) if isinstance(r, dict)}
    err = hypr.lua(_lua_code(cfg, _anim(), focused.get("name", ""), own))
    if not err and cfg["bar"]:
        err = set_caelestia_shown(cfg["per"])
    return err


def turn_off() -> str:
    """Remove ours and reload Hyprland's config so your own binds for those keys come back."""
    if not hypr.available():
        return ""
    hypr.lua("if LUMI and LUMI.ws then for _, b in pairs(LUMI.ws.binds or {}) do pcall(function() b:remove() end) "
             "end for _, r in pairs(LUMI.ws.rules or {}) do pcall(function() r:set_enabled(false) end) end "
             "LUMI.ws.binds, LUMI.ws.rules = {}, {} end")
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
