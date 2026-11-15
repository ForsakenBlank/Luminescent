"""Workspace sets: the Lua that runs inside Hyprland, driven here by a pretend Hyprland with two monitors.

Needs a `lua` program (any 5.x or luajit); skipped without one. Run: .venv/bin/python -m pytest tests -q
"""
import shutil
import subprocess

import pytest

from core import workspaces as wsp

LUA = shutil.which("lua") or shutil.which("luajit") or shutil.which("lua5.4")

# a tiny Hyprland: monitors A and B, workspaces live on a monitor; a workspace is a number or "name:x".
# The log says what was focused and with which animation style: "4@A(slide right)".
MOCK = r"""
local mons = { A = { name = "A" }, B = { name = "B" } }
local spaces, list = {}, {}
local focused = mons.A
local style = "slide"
log = {}
local function ws(key, mon)
  if not spaces[key] then
    local w = { special = false, monitor = mon }
    if type(key) == "number" then w.id = key w.name = tostring(key) else w.name = key:sub(6) end
    spaces[key] = w
    list[#list + 1] = w
  end
  return spaces[key]
end
hl = { dsp = { window = {}, workspace = {} } }
function hl.get_active_monitor() return focused end
function hl.get_active_workspace() return focused.active_workspace end
function hl.get_workspace(id) return spaces[id] end
function hl.get_workspaces() return list end
function hl.animation(a) style = a.enabled and a.style or "none" end
function hl.timer(fn) pending = fn return { set_enabled = function() pending = nil end } end
binds = {}
function hl.bind(keys, fn) binds[keys] = { fn = fn, on = true } local b = binds[keys]
  return { set_enabled = function(_, v) b.on = v end } end
function press(keys) local b = binds[keys] if b and b.on then b.fn() end end
function hl.unbind() end
function hl.workspace_rule(r) rules[#rules + 1] = r.workspace .. "@" .. tostring(r.monitor) return { set_enabled = function() end } end
function hl.dsp.focus(t) return { kind = "focus", id = t.workspace } end
function hl.dsp.window.move(t) return { kind = "movewin", id = t.workspace, window = t.window, follow = t.follow } end
function hl.dsp.workspace.move(t) return { kind = "movews", id = t.workspace, monitor = t.monitor } end
function hl.dsp.workspace.change_id(t) return { kind = "renumber", id = t.workspace, to = t.id } end
function hl.dispatch(d)
  if d.kind == "movews" then
    local w = spaces[d.id]
    if w then w.monitor = mons[d.monitor] log[#log + 1] = "ws" .. d.id .. ">" .. d.monitor end
    return
  end
  if d.kind == "renumber" then
    local w = spaces[d.id]
    spaces[d.id] = nil spaces[d.to] = w w.id = d.to w.name = tostring(d.to)
    log[#log + 1] = d.id .. "=>" .. d.to
    return
  end
  if d.window then log[#log + 1] = d.window .. ">" .. d.id .. (d.follow == false and " quiet" or "") return end
  local w = ws(d.id, focused)
  focused = w.monitor                       -- like Hyprland: focusing a workspace goes to the monitor it is on
  focused.active_workspace = w
  log[#log + 1] = (d.kind == "movewin" and "win>" or "") .. (w.id or w.name) .. "@" .. focused.name .. "(" .. style .. ")"
end
rules = {}
function at(mon, key) focused = mons[mon] focused.active_workspace = ws(key, focused) end
function tick() if pending then local f = pending pending = nil f() end end
function out() print(table.concat(log, " ")) log = {} end
"""

ANIM = {"enabled": True, "speed": 5.0, "bezier": "quick", "style": "slide"}


def run(cfg: dict, script: str, order=("A", "B"), own=(), stray=(), anim=None) -> list[str]:
    code = MOCK + wsp._lua_code(wsp.config(dict(cfg, on=True)), anim or ANIM, list(order), set(own), list(stray))
    res = subprocess.run([LUA, "-"], input=code + script, capture_output=True, text=True, timeout=20)
    assert res.returncode == 0, res.stderr
    return res.stdout.strip().splitlines()


needs_lua = pytest.mark.skipif(not LUA, reason="no lua program")


@needs_lua
def test_it_always_slides_the_way_you_pressed():
    lines = run({"per": 4, "sets": 2}, """
    local W = LUMI.ws
    at("A", 1) W.step(-1) out()           -- Left on 1 goes round to 4 and still comes in from the left
    at("A", 4) W.step(1) out()            -- Right on 4 goes round to 1 and comes in from the right
    at("A", 2) W.step(1) out()
    at("A", 2) W.set(-1) out()            -- Up from the first set goes round to the last: from the top, no fade
    at("A", 2) W.set(1) out()
    tick() at("A", 1) hl.dispatch(hl.dsp.focus({ workspace = 3 })) out()   -- afterwards your own style is back
    """)
    assert lines == ["4@A(slide left)", "1@A(slide right)", "3@A(slide right)", "6@A(slide top)",
                     "6@A(slide bottom)", "3@A(slide)"]


@needs_lua
def test_each_monitor_keeps_its_own_workspaces():
    lines = run({"per": 4, "sets": 2}, """
    local W = LUMI.ws
    at("A", 3) W.step(1) out()
    at("B", 9) W.step(1) W.step(1) out()  -- B has 9-16
    at("B", 9) W.step(-1) out()           -- round to 12 on B
    at("B", 10) W.set(1) out()            -- 10 -> 14: the same spot in B's second set
    """)
    assert lines == ["4@A(slide right)", "10@B(slide right) 11@B(slide right)", "12@B(slide left)",
                     "14@B(slide bottom)"]


@needs_lua
def test_named_workspaces_are_part_of_the_round():
    lines = run({"per": 4, "sets": 1}, """
    local W = LUMI.ws
    at("A", "name:gaming") at("A", 4)
    W.step(1) out()                       -- 4 -> gaming
    W.step(1) out()                       -- gaming -> 1
    W.step(-1) out()                      -- 1 -> gaming
    W.step(-1) out()                      -- gaming -> 4
    at("B", 5) W.step(-1) out()           -- gaming is on A: B just goes round
    """)
    assert lines == ["gaming@A(slide right)", "1@A(slide right)", "gaming@A(slide left)", "4@A(slide left)",
                     "8@B(slide left)"]
    off = run({"per": 4, "sets": 1, "named": False}, """
    at("A", "name:gaming") at("A", 4) LUMI.ws.step(1) out()
    at("A", "name:gaming") LUMI.ws.step(1) out()      -- still a way out of it: back to where you were
    """)
    assert off == ["1@A(slide right)", "1@A(slide right)"]


@needs_lua
def test_a_workspace_on_the_wrong_screen_is_brought_back():
    lines = run({"per": 4, "sets": 1}, """
    at("B", 4) at("A", 3)                 -- 4 belongs to A's block but is open on B
    LUMI.ws.step(1) out()
    """)
    assert lines[0] == "ws4>A 4@A(slide right)"


@needs_lua
def test_wrap_choices_and_other_looks():
    assert run({"per": 4, "sets": 2, "split": False}, 'at("B", 4) LUMI.ws.step(1) out()')[0] == "1@B(slide right)"
    assert run({"per": 4, "sets": 2, "wrap": "stop"},
               'at("A", 4) LUMI.ws.step(1) at("A", 2) LUMI.ws.set(-1) print(#log)')[0] == "0"
    assert run({"per": 4, "sets": 2, "wrap": "next_set"}, 'at("B", 12) LUMI.ws.step(1) out()')[0] == "13@B(slide right)"
    assert run({"per": 4, "sets": 2, "wrap": "next_set"},
               'at("B", 16) LUMI.ws.step(1) out()')[0] == "9@B(slide right)"
    assert run({"per": 4, "sets": 1}, 'at("A", 1) LUMI.ws.step(1, true) out()')[0] == "win>2@A(slide right)"
    assert run({"per": 4, "sets": 1, "anim": "fade"}, 'at("A", 1) LUMI.ws.step(-1) out()')[0] == "4@A(fade)"
    assert run({"per": 4, "sets": 1, "anim": "none"}, 'at("A", 1) LUMI.ws.step(-1) out()')[0] == "4@A(none)"
    assert run({"per": 4, "sets": 1, "anim": "hypr"}, 'at("A", 1) LUMI.ws.step(-1) out()')[0] == "4@A(slide)"
    fade20 = dict(ANIM, style="slidefade 20%")       # your own slidefade keeps its look, with a direction
    assert run({"per": 4, "sets": 1}, 'at("A", 1) LUMI.ws.step(-1) out()', anim=fade20)[0] == "4@A(slidefade left 20%)"


@needs_lua
def test_old_settings_still_load():
    cfg = wsp.config({"on": True, "wrap": "slide", "vertical": "slidevert"})     # 1.9.0's names
    assert cfg["wrap"] == "round" and cfg["anim"] == "slide" and "vertical" not in cfg


@needs_lua
def test_rules_go_to_the_right_monitor():
    lines = run({"per": 2, "sets": 1}, 'print(table.concat(rules, " "))', own={"1"})
    assert lines[0] == "2@A 3@B 4@B"


@needs_lua
def test_changing_a_setting_reuses_the_binds():
    first = wsp._lua_code(wsp.config({"on": True, "per": 4, "sets": 2}), ANIM, ["A", "B"], set(), [])
    second = wsp._lua_code(wsp.config({"on": True, "per": 2, "sets": 1}), ANIM, ["A", "B"], set(), [])
    script = """
    at("A", 2) press("SUPER + CONTROL + Right") out()
    at("A", 2) press("SUPER + CONTROL + Down") print(#log)      -- one set now: Down is switched off
    """
    res = subprocess.run([LUA, "-"], input=MOCK + first + "\n" + second + script, capture_output=True, text=True)
    assert res.returncode == 0, res.stderr
    assert res.stdout.splitlines() == ["1@A(slide right)", "0"]  # 2 per set now: Right on 2 goes round to 1


@needs_lua
def test_number_keys_pick_inside_the_set():
    lines = run({"per": 4, "sets": 2, "digits": True}, """
    at("B", 14) press("SUPER + CONTROL + code:10") out()             -- the 1 key in B's second set
    at("A", 1) press("SUPER + CONTROL + SHIFT + code:13") out()      -- the 4 key, with the window
    print(binds["SUPER + CONTROL + code:14"] == nil)                 -- 5 is not in a set of 4
    """)
    assert lines == ["13@B(slide left)", "win>4@A(slide right)", "true"]


@needs_lua
def test_turning_sets_on_keeps_windows_on_their_monitor():
    cfg = wsp.config({"per": 4, "sets": 2})
    spaces = [{"id": 4, "monitor": "B"}, {"id": 1, "monitor": "A"}, {"id": -98, "monitor": "A"},
              {"id": 9, "monitor": "B"}, {"id": 40, "monitor": "A"}, {"id": 1, "monitor": "A"}]
    stray = wsp._stray(cfg, ["A", "B"], spaces, [])
    lines = run({"per": 4, "sets": 2}, "out()", stray=stray)        # 4 is open on B: it becomes B's 12, in place
    assert lines == [] or lines == [""]
    code = MOCK + 'at("B", 4) at("A", 1)\n' + wsp._lua_code(cfg | {"on": True}, ANIM, ["A", "B"], set(), stray) + "out()"
    res = subprocess.run([LUA, "-"], input=code, capture_output=True, text=True)
    assert res.returncode == 0, res.stderr
    assert res.stdout.strip() == "4=>12"
    # 12 is taken already: the windows of 4 go over to it without pulling you along
    spaces = [{"id": 4, "monitor": "B"}, {"id": 12, "monitor": "B"}]
    clients = [{"address": "0xabc", "workspace": {"id": 4}}, {"address": "0xdef", "workspace": {"id": 12}}]
    stray = wsp._stray(cfg, ["A", "B"], spaces, clients)
    assert len(stray) == 1 and "0xabc" in stray[0] and "follow = false" in stray[0] and "workspace = 12" in stray[0]


def test_monitor_order_and_layout():
    mons = [{"name": "HDMI-A-2", "x": 1920, "y": 0}, {"name": "HDMI-A-3", "x": 0, "y": 0}]
    assert wsp.monitor_order(mons, []) == ["HDMI-A-3", "HDMI-A-2"]                     # left to right
    rules = [{"workspaceString": "name:gaming", "monitor": "HDMI-A-3"},
             {"workspaceString": "1", "monitor": "HDMI-A-2"}]
    order = wsp.monitor_order(mons, rules)
    assert order == ["HDMI-A-2", "HDMI-A-3"]                                           # your rule for 1 wins
    assert wsp.monitor_order(mons, rules, "HDMI-A-3") == ["HDMI-A-3", "HDMI-A-2"]
    assert wsp.monitor_order(mons, rules, "gone") == order
    cfg = wsp.config({"per": 4, "sets": 2})
    assert wsp.layout(cfg, order)[1] == ("HDMI-A-3", [[9, 10, 11, 12], [13, 14, 15, 16]])
    assert wsp.layout(wsp.config({"per": 3, "sets": 1, "split": False}), order) == [("", [[1, 2, 3]])]
