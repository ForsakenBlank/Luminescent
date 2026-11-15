"""Workspace sets: the Lua that runs inside Hyprland, driven here by a pretend Hyprland with two monitors.

Needs a `lua` program (any 5.x or luajit); skipped without one. Run: .venv/bin/python -m pytest tests -q
"""
import shutil
import subprocess

import pytest

from core import workspaces as wsp

LUA = shutil.which("lua") or shutil.which("luajit") or shutil.which("lua5.4")

# a tiny Hyprland: monitors A (right, owns 1-3 by the user's rules) and B, workspaces live on a monitor
MOCK = r"""
local mons = { A = { name = "A" }, B = { name = "B" } }
local spaces = {}
local focused = mons.A
log = {}
local function ws(id, mon)
  if not spaces[id] then spaces[id] = { id = id, name = tostring(id), special = false, monitor = mon } end
  return spaces[id]
end
hl = { dsp = { window = {}, workspace = {} } }
function hl.get_active_monitor() return focused end
function hl.get_active_workspace() return focused.active_workspace end
function hl.get_workspace(id) return spaces[id] end
function hl.animation(a) log[#log + 1] = "anim:" .. (a.enabled and a.style or "none") end
function hl.timer(fn) pending = fn return { set_enabled = function() pending = nil end } end
binds = {}
function hl.bind(keys, fn) binds[keys] = { fn = fn, on = true } local b = binds[keys]
  return { set_enabled = function(_, v) b.on = v end } end
function press(keys) local b = binds[keys] if b and b.on then b.fn() end end
function hl.unbind() end
function hl.workspace_rule(r) rules[#rules + 1] = r.workspace .. "@" .. tostring(r.monitor) return { set_enabled = function() end } end
function hl.dsp.focus(t) return { kind = "focus", id = t.workspace } end
function hl.dsp.window.move(t) return { kind = "movewin", id = t.workspace } end
function hl.dsp.workspace.move(t) return { kind = "movews", id = t.workspace, monitor = t.monitor } end
function hl.dispatch(d)
  if d.kind == "movews" then
    local w = spaces[d.id]
    if w then w.monitor = mons[d.monitor] log[#log + 1] = "ws" .. d.id .. ">" .. d.monitor end
    return
  end
  local w = ws(d.id, focused)
  focused = w.monitor                       -- like Hyprland: focusing a workspace goes to the monitor it is on
  focused.active_workspace = w
  log[#log + 1] = (d.kind == "movewin" and "win>" or "") .. d.id .. "@" .. focused.name
end
rules = {}
function at(mon, id) focused = mons[mon] focused.active_workspace = ws(id, focused) end
function named(mon) focused = mons[mon] focused.active_workspace = { name = "gaming", special = false, monitor = focused } end
function tick() if pending then local f = pending pending = nil f() end end
function out() print(table.concat(log, " ")) log = {} end
"""


def run(cfg: dict, script: str, order=("A", "B"), own=(), stray=()) -> list[str]:
    anim = {"enabled": True, "speed": 5.0, "bezier": "quick", "style": "slide"}
    code = MOCK + wsp._lua_code(wsp.config(dict(cfg, on=True)), anim, list(order), set(own), list(stray)) + script
    res = subprocess.run([LUA, "-"], input=code, capture_output=True, text=True, timeout=20)
    assert res.returncode == 0, res.stderr
    return res.stdout.strip().splitlines()


needs_lua = pytest.mark.skipif(not LUA, reason="no lua program")


@needs_lua
def test_each_monitor_keeps_its_own_workspaces():
    lines = run({"per": 4, "sets": 2}, """
    local W = LUMI.ws
    at("A", 3) W.step(1) out()            -- 3 -> 4 stays on A
    at("B", 9) W.step(1) W.step(1) out()  -- B has 9-16
    at("B", 9) W.step(-1) out()           -- round to 12 on B, as a fade
    tick() out()                          -- the timer puts the slide back
    at("B", 10) W.set(1) out()            -- 10 -> 14: the same spot in B's second set
    at("A", 2) W.set(-1) out()            -- set 1 -> set 2 the short way round: a fade, not a backwards slide
    """)
    assert lines[0] == "4@A"
    assert lines[1] == "10@B 11@B"
    assert lines[2] == "anim:fade 12@B"
    assert lines[3] == "anim:slide"
    assert lines[4] == "anim:slidevert 14@B"
    assert lines[5] == "anim:fade 6@A"


@needs_lua
def test_a_workspace_on_the_wrong_screen_is_brought_back():
    lines = run({"per": 4, "sets": 1}, """
    local W = LUMI.ws
    at("B", 4) at("A", 3)                 -- 4 belongs to A's block but is open on B
    W.step(1) out()
    """)
    assert lines[0] == "ws4>A 4@A"


@needs_lua
def test_named_workspace_goes_back_to_the_last_one():
    lines = run({"per": 4, "sets": 1}, """
    local W = LUMI.ws
    at("A", 2) W.step(1) out()
    named("A") W.step(1) out()            -- from "gaming": back to 3, not on to 4
    named("B") W.step(-1) out()           -- B was never used: its first workspace
    """)
    assert lines == ["3@A", "3@A", "5@B"]


@needs_lua
def test_slide_comes_back_before_a_normal_step():
    lines = run({"per": 3, "sets": 1, "wrap": "fade"}, """
    local W = LUMI.ws
    at("A", 3) W.step(1) W.step(1) out()  -- 3 -> 1 fades, then 1 -> 2 at once: it must slide again
    """)
    assert lines[0] == "anim:fade 1@A anim:slide 2@A"


@needs_lua
def test_shared_pool_and_wrap_choices():
    assert run({"per": 4, "sets": 2, "split": False}, 'at("B", 4) LUMI.ws.step(1) out()')[0] == "anim:fade 1@B"
    assert run({"per": 4, "sets": 2, "wrap": "stop"}, 'at("A", 4) LUMI.ws.step(1) print(#log)')[0] == "0"
    assert run({"per": 4, "sets": 2, "wrap": "next_set"}, 'at("B", 12) LUMI.ws.step(1) out()')[0] == "13@B"
    assert run({"per": 4, "sets": 2, "wrap": "next_set"},
               'at("B", 16) LUMI.ws.step(1) out()')[0] == "anim:fade 9@B"
    assert run({"per": 4, "sets": 1, "move_keys": True}, 'at("A", 1) LUMI.ws.step(1, true) out()')[0] == "win>2@A"


@needs_lua
def test_rules_go_to_the_right_monitor_and_strays_move():
    lines = run({"per": 2, "sets": 1}, 'print(table.concat(rules, " "))', own={"1"}, stray=[(2, "A")])
    assert lines[0] == "2@A 3@B 4@B"


def test_monitor_order_and_strays():
    mons = [{"name": "HDMI-A-2", "x": 1920, "y": 0}, {"name": "HDMI-A-3", "x": 0, "y": 0}]
    assert wsp.monitor_order(mons, []) == ["HDMI-A-3", "HDMI-A-2"]                     # left to right
    rules = [{"workspaceString": "name:gaming", "monitor": "HDMI-A-3"},
             {"workspaceString": "1", "monitor": "HDMI-A-2"}]
    order = wsp.monitor_order(mons, rules)
    assert order == ["HDMI-A-2", "HDMI-A-3"]                                           # your rule for 1 wins
    cfg = wsp.config({"per": 4, "sets": 2})
    spaces = [{"id": 4, "monitor": "HDMI-A-3"}, {"id": 1, "monitor": "HDMI-A-2"}, {"id": -98, "monitor": "HDMI-A-2"},
              {"id": 9, "monitor": "HDMI-A-3"}, {"id": 40, "monitor": "HDMI-A-2"}]
    assert wsp._stray(cfg, order, spaces) == [(4, "HDMI-A-2")]
    assert wsp.layout(cfg, order)[1] == ("HDMI-A-3", [[9, 10, 11, 12], [13, 14, 15, 16]])
    assert wsp.layout(wsp.config({"per": 3, "sets": 1, "split": False}), order) == [("", [[1, 2, 3]])]


@needs_lua
def test_changing_a_setting_reuses_the_binds():
    anim = {"enabled": True, "speed": 5.0, "bezier": "quick", "style": "slide"}
    first = wsp._lua_code(wsp.config({"on": True, "per": 4, "sets": 2}), anim, ["A", "B"], set(), [])
    second = wsp._lua_code(wsp.config({"on": True, "per": 2, "sets": 1}), anim, ["A", "B"], set(), [])
    script = """
    at("A", 2) press("SUPER + CONTROL + Right") out()
    at("A", 2) press("SUPER + CONTROL + Down") print(#log)      -- one set now: Down is switched off
    """
    res = subprocess.run([LUA, "-"], input=MOCK + first + "\n" + second + script, capture_output=True, text=True)
    assert res.returncode == 0, res.stderr
    assert res.stdout.split() == ["anim:fade", "1@A", "0"]       # 2 per set now: Right on 2 goes round to 1


@needs_lua
def test_number_keys_pick_inside_the_set():
    lines = run({"per": 4, "sets": 2, "digits": True}, """
    at("B", 14) press("SUPER + CONTROL + code:10") out()             -- the 1 key in B's second set
    at("A", 1) press("SUPER + CONTROL + SHIFT + code:13") out()      -- the 4 key, with the window
    print(binds["SUPER + CONTROL + code:14"] == nil)                 -- 5 is not in a set of 4
    """)
    assert lines == ["13@B", "win>4@A", "true"]


def test_first_monitor_choice():
    mons = [{"name": "L", "x": 0}, {"name": "R", "x": 1920}]
    rules = [{"workspaceString": "1", "monitor": "R"}]
    assert wsp.monitor_order(mons, rules) == ["R", "L"]
    assert wsp.monitor_order(mons, rules, "L") == ["L", "R"]
    assert wsp.monitor_order(mons, rules, "gone") == ["R", "L"]
