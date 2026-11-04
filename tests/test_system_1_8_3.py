"""The moving-wallpaper switch and hypridle.conf editing. Run: python tests/test_system_1_8_3.py"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from core import wallpaper, syscontrol as sc  # noqa: E402


def test_finds_only_what_luminescent_starts():
    assert wallpaper._is_animated([b"/usr/bin/mpvpaper", b"-o", b"no-audio --loop", b"DP-1", b"a.mp4", b""])
    assert wallpaper._is_animated([b"linux-wallpaperengine", b"--screen-root", b"DP-1", b"--bg", b"/p", b""])
    assert not wallpaper._is_animated([b"mpvpaper", b"-o", b"loop", b"DP-1", b"mine.mp4", b""])
    assert not wallpaper._is_animated([b"linux-wallpaperengine", b"/p", b""])
    assert not wallpaper._is_animated([b"mpv", b"a.mp4", b""])


def test_picture_stops_leftover_video():
    killed = []
    real = wallpaper._my_processes, os.kill
    wallpaper._my_processes = lambda: iter([(11, [b"mpvpaper", b"-o", b"no-audio --loop", b"DP-1", b"a"]),
                                            (12, [b"bash", b""])])
    os.kill = lambda pid, sig: killed.append(pid)
    try:
        wallpaper.stop_animated(leftovers=False)
        assert killed == []
        wallpaper.stop_animated()
        assert killed == [11]
    finally:
        wallpaper._my_processes, os.kill = real


CONF = """general {
    lock_cmd = hyprlock
}

listener {
    timeout = 300
    on-timeout = loginctl lock-session
}

listener {
    timeout = 330
    on-timeout = hyprctl dispatch dpms off
    on-resume = hyprctl dispatch dpms on
}

listener {
    timeout = 60
    on-timeout = notify-send hello
}
"""


def test_hypridle():
    assert sc.hypridle_minutes(CONF, "lock") == 5
    assert sc.hypridle_minutes(CONF, "sleep") == 0
    changed = sc.hypridle_with(CONF, "lock", 10)
    assert sc.hypridle_minutes(changed, "lock") == 10 and "timeout = 330" in changed and "notify-send" in changed
    gone = sc.hypridle_with(CONF, "screen", 0)
    assert sc.hypridle_minutes(gone, "screen") == 0 and "lock_cmd" in gone and "notify-send" in gone
    added = sc.hypridle_with(CONF, "sleep", 30)
    assert sc.hypridle_minutes(added, "sleep") == 30 and "systemctl suspend" in added


def test_caelestia_keeps_your_own_steps():
    import json
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        real, sc._CAEL_SHELL = sc._CAEL_SHELL, os.path.join(d, "shell.json")
        try:
            with open(sc._CAEL_SHELL, "w") as f:
                json.dump({"bar": {"x": 1}, "general": {"idle": {"timeouts": [
                    {"timeout": 60, "idleAction": ["notify-send", "hi"]},
                    {"timeout": 300, "idleAction": "dpms off", "returnAction": "dpms on"}]}}}, f)
            assert sc._cael_minutes("screen") == 5 and sc._cael_minutes("lock") == 0
            assert sc._cael_set_minutes("lock", 3) == "" and sc._cael_set_minutes("screen", 10) == ""
            d2 = json.load(open(sc._CAEL_SHELL))
            ts = d2["general"]["idle"]["timeouts"]
            assert d2["bar"] == {"x": 1} and [t["timeout"] for t in ts] == [60, 180, 600]
            assert ts[2]["returnAction"] == "dpms on"
            assert sc._cael_set_minutes("screen", 0) == "" and sc._cael_minutes("screen") == 0
            assert sc._cael_minutes("lock") == 3
        finally:
            sc._CAEL_SHELL = real


def test_idle_kind():
    assert sc._idle_kind("lock") == "lock" and sc._idle_kind("loginctl unlock-session") == ""
    assert sc._idle_kind(["systemctl", "suspend-then-hibernate"]) == "sleep"
    assert sc._idle_kind("dpms off") == "screen" and sc._idle_kind("dpms on") == ""


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
            print("ok", name)
