"""
Theme profiles (data/themes.json).

    {"active": id,
     "folders": [{"id", "name", "colour", "collapsed"}],
     "themes":  [{"id", "name", "folder", "builtin", "colours": {role: "#hex"}, "font_family", "radius",
                  "glow", "icon", "background_image", "image_opacity", "wallpaper", "profile"}]}

The built-in Dark and Light themes cannot be deleted, so there is always at
least one.  `apply()` makes a theme the active palette and emits `applied`;
the launcher restyles everything, switches the settings profile and the
wallpaper from there.
"""
import os
import copy
import uuid
import logging

from PyQt6.QtCore import QObject, pyqtSignal

from core import DATA_DIR, save_json, load_json, theme

log = logging.getLogger("themes")

PATH = os.path.join(DATA_DIR, "themes.json")
LOOK_KEYS = ("font_family", "radius", "glow", "background_image", "image_opacity")


def _builtin(tid: str, name: str) -> dict:
    return {"id": tid, "name": name, "folder": "", "builtin": True, "colours": dict(theme.PRESETS[tid]),
            "font_family": "Geist", "radius": 10, "glow": True, "icon": "", "background_image": "",
            "image_opacity": 0.35, "wallpaper": "", "profile": ""}


def new_theme(name: str, base: dict | None = None) -> dict:
    t = copy.deepcopy(base) if base else _builtin("dark", name)
    t.update({"id": uuid.uuid4().hex[:10], "name": name, "builtin": False})
    return t


class ThemeStore(QObject):
    applied = pyqtSignal(dict)       # the theme that is now active
    changed = pyqtSignal()           # list / folders / names changed

    def __init__(self, settings=None):
        super().__init__()
        self.data = self._load(settings)

    # ── persistence ────────────────────────────────────────────────────────
    def _load(self, settings) -> dict:
        d = load_json(PATH, None)
        if not isinstance(d, dict) or not isinstance(d.get("themes"), list):
            d = self._first_run(settings)
            try:
                save_json(PATH, d, indent=2)
            except OSError:
                log.exception("could not write themes.json")
        d.setdefault("folders", [])
        d["themes"] = [t for t in d["themes"] if isinstance(t, dict) and t.get("id")]
        for tid, name in (("dark", "Dark"), ("light", "Light")):
            if not any(t["id"] == tid for t in d["themes"]):
                d["themes"].insert(0 if tid == "dark" else 1, _builtin(tid, name))
        migrated = False
        for t in d["themes"]:
            base = _builtin("dark", t.get("name", "Theme"))
            for k, v in base.items():
                t.setdefault(k, v)
            cols = dict(t.get("colours") or {})
            old = theme.OLD_PRESETS.get(t["id"]) if t.get("builtin") else None
            if old and all(str(cols.get(k, "")).upper() == v.upper() for k, v in old.items()):
                # an untouched built-in from before the redesign: move it to the new colours and type
                cols = dict(theme.PRESETS[t["id"]])
                if t.get("font_family") in ("", "Segoe UI"):
                    t["font_family"] = "Geist"
                migrated = True
            if not cols.get("primary_text"):
                cols["primary_text"] = theme.derive_primary_text(cols)
            t["colours"] = {**theme.PRESETS["dark"], **cols}
        if migrated:
            try:
                save_json(PATH, d, indent=2)
            except OSError:
                log.exception("could not save themes.json")
        if not any(t["id"] == d.get("active") for t in d["themes"]):
            d["active"] = "dark"
        return d

    @staticmethod
    def _first_run(settings) -> dict:
        """Turn the old Settings → Appearance (mode + custom colours) into theme profiles."""
        d = {"active": "dark", "folders": [], "themes": [_builtin("dark", "Dark"), _builtin("light", "Light")]}
        old = (getattr(settings, "raw_saved", {}) or {}).get("appearance") or {}
        mode = old.get("mode") or ("custom" if old.get("primary") else "dark")
        look = {k: old[k] for k in ("font_family", "radius") if k in old}
        if mode == "custom":
            custom = new_theme("My old colours")
            custom["colours"].update({k: v for k, v in old.items() if k in theme.PRESETS["dark"]})
            if "on_primary" not in old and "bg_base" in old:
                custom["colours"]["on_primary"] = old["bg_base"]
            custom.update(look)
            d["themes"].append(custom)
            # the old default "custom" colours were never chosen on purpose: keep them, start on Dark
            d["active"] = custom["id"] if old.get("mode") == "custom" else "dark"
        elif mode in ("dark", "light"):
            d["active"] = mode
            for t in d["themes"]:
                if t["id"] == mode:
                    t.update(look)
        return d

    def save(self):
        try:
            save_json(PATH, self.data, indent=2)
        except OSError:
            log.exception("could not save themes.json")

    # ── access ─────────────────────────────────────────────────────────────
    def themes(self) -> list[dict]:
        return self.data["themes"]

    def folders(self) -> list[dict]:
        return self.data["folders"]

    def get(self, tid: str) -> dict | None:
        return next((t for t in self.data["themes"] if t["id"] == tid), None)

    def active(self) -> dict:
        return self.get(self.data["active"]) or self.data["themes"][0]

    @staticmethod
    def palette_values(t: dict) -> dict:
        return {**t.get("colours", {}), **{k: t.get(k) for k in LOOK_KEYS if k in t}}

    # ── changes ────────────────────────────────────────────────────────────
    def use_active(self):
        """Make the saved active theme the palette (start-up)."""
        t = self.active()
        theme.use(self.palette_values(t))
        theme.set_module_accents(t.get("modules"))

    def apply(self, tid: str):
        t = self.get(tid)
        if t is None:
            return
        self.data["active"] = tid
        self.save()
        theme.use(self.palette_values(t))
        theme.set_module_accents(t.get("modules"))
        log.info("theme applied: %s", t["name"])
        self.applied.emit(t)

    def update_theme(self, t: dict):
        """Store an edited theme; re-apply it if it is the active one."""
        for i, old in enumerate(self.data["themes"]):
            if old["id"] == t["id"]:
                self.data["themes"][i] = t
        self.save()
        self.changed.emit()
        if t["id"] == self.data["active"]:
            self.apply(t["id"])

    def add(self, t: dict):
        self.data["themes"].append(t)
        self.save()
        self.changed.emit()

    def duplicate(self, tid: str) -> dict | None:
        t = self.get(tid)
        if t is None:
            return None
        c = new_theme(f"{t['name']} copy", t)
        self.add(c)
        return c

    def rename(self, tid: str, name: str):
        t = self.get(tid)
        if t and name.strip():
            t["name"] = name.strip()
            self.save()
            self.changed.emit()

    def can_delete(self, tid: str) -> bool:
        t = self.get(tid)
        return bool(t) and not t.get("builtin") and len(self.data["themes"]) > 1

    def delete(self, tid: str):
        if not self.can_delete(tid):
            return
        was_active = self.data["active"] == tid
        self.data["themes"] = [t for t in self.data["themes"] if t["id"] != tid]
        self.save()
        self.changed.emit()
        if was_active:
            self.apply(self.data["themes"][0]["id"])

    def move_to_folder(self, tid: str, folder_id: str):
        t = self.get(tid)
        if t:
            t["folder"] = folder_id or ""
            self.save()
            self.changed.emit()

    def add_folder(self, name: str, colour: str) -> dict:
        f = {"id": uuid.uuid4().hex[:10], "name": name.strip() or "Folder", "colour": colour, "collapsed": False}
        self.data["folders"].append(f)
        self.save()
        self.changed.emit()
        return f

    def update_folder(self, fid: str, **kw):
        for f in self.data["folders"]:
            if f["id"] == fid:
                f.update(kw)
        self.save()
        self.changed.emit()

    def delete_folder(self, fid: str):
        self.data["folders"] = [f for f in self.data["folders"] if f["id"] != fid]
        for t in self.data["themes"]:
            if t.get("folder") == fid:
                t["folder"] = ""
        self.save()
        self.changed.emit()

    def next_theme(self) -> dict | None:
        """The theme after the active one: within the active theme's folder (or the built-in / loose themes
        it sits with), wrapping round."""
        act = self.active()
        folder = act.get("folder") or ""
        folders = {f["id"] for f in self.folders()}
        if folder in folders:
            ring = [t for t in self.themes() if t.get("folder") == folder]
        else:
            ring = [t for t in self.themes() if not t.get("folder") or t["folder"] not in folders]
        if len(ring) < 2:
            ring = self.themes()
        ids = [t["id"] for t in ring]
        i = ids.index(act["id"]) if act["id"] in ids else -1
        return ring[(i + 1) % len(ring)] if ring else None

    # ── made from one colour ───────────────────────────────────────────────
    def from_accent(self, accent: str, light: bool, name: str = "") -> dict:
        """A new theme whose colours all go with `accent` (added, not applied)."""
        t = new_theme(name or f"{accent.upper()} {'light' if light else 'dark'}")
        t["colours"] = {**theme.PRESETS["light" if light else "dark"], **theme.generate(accent, light)}
        self.add(t)
        return t

    def from_colours(self, name: str, roles: dict) -> str:
        """A theme from another app's colours ({"primary", "bg", "card", "text", "muted", "border"}). A theme
        with the same name is updated instead of adding another. Returns its id ('' without an accent)."""
        accent = roles.get("primary") or ""
        if not accent.startswith("#"):
            return ""
        bg = roles.get("bg") or ""
        light = bool(bg) and theme.luminance(bg) > 140
        cols = {**theme.PRESETS["light" if light else "dark"], **theme.generate(accent, light)}
        for role, keys in (("bg", ("bg_base",)), ("card", ("bg_card", "folder_color")), ("text", ("text_primary",)),
                           ("muted", ("text_secondary",)), ("border", ("border",))):
            if roles.get(role, "").startswith("#"):
                for k in keys:
                    cols[k] = roles[role]
        if roles.get("bg", "").startswith("#"):
            cols["bg_frame"] = theme.mix(roles["bg"], "#000000" if not light else "#FFFFFF", 0.25)
            cols["bg_header"] = theme.mix(roles["bg"], cols["bg_card"], 0.5)
            cols["bg_card_hover"] = theme.mix(cols["bg_card"], cols["text_primary"], 0.06)
        old = next((t for t in self.themes() if t.get("name") == name and not t.get("builtin")), None)
        if old is not None:
            t = copy.deepcopy(old)
            t["colours"] = cols
            self.update_theme(t)
            return t["id"]
        t = new_theme(name)
        t["colours"] = cols
        self.add(t)
        return t["id"]

    def set_accent(self, tid: str, accent: str):
        """Give a theme a new accent colour; its accent shades follow."""
        t = self.get(tid)
        if t is None:
            return
        t = copy.deepcopy(t)
        t["colours"].update(theme.accent_roles(accent.upper(), t["colours"].get("bg_base", "#131317")))
        if t["colours"].get("folder_border"):
            t["colours"]["folder_border"] = t["colours"]["primary_text"]
        self.update_theme(t)

    # ── sharing ────────────────────────────────────────────────────────────
    def export(self, tid: str, path: str):
        """Write one theme to a .lumtheme file (JSON). Its background image and card icon go inside it."""
        import base64
        t = self.get(tid)
        if t is None:
            raise ValueError("no such theme")
        out = {k: copy.deepcopy(v) for k, v in t.items() if k not in ("id", "builtin", "folder", "profile")}
        files = {}
        for key in ("background_image", "icon"):
            p = t.get(key) or ""
            if p and os.path.isfile(p) and os.path.getsize(p) < 20_000_000:
                with open(p, "rb") as f:
                    files[key] = {"name": os.path.basename(p), "data": base64.b64encode(f.read()).decode("ascii")}
            out[key] = ""
        out["wallpaper"] = ""                  # a path on this PC means nothing on another one
        save_json(path, {"luminescent_theme": 1, "theme": out, "files": files}, indent=1)

    def import_file(self, path: str) -> dict:
        """Add the theme in a .lumtheme file. Raises ValueError if it is not one."""
        import base64
        d = load_json(path, None)
        if not isinstance(d, dict) or not isinstance(d.get("theme"), dict) or "luminescent_theme" not in d:
            raise ValueError("This file is not a Luminescent theme (.lumtheme).")
        src = d["theme"]
        t = new_theme(str(src.get("name") or "Imported theme"))
        for k in ("colours", "font_family", "radius", "glow", "image_opacity", "modules"):
            if k in src:
                t[k] = copy.deepcopy(src[k])
        t["colours"] = {**theme.PRESETS["dark"], **{k: v for k, v in (t.get("colours") or {}).items()
                                                    if isinstance(v, str) and v.startswith("#")}}
        folder = os.path.join(DATA_DIR, "theme-images")
        for key, f in (d.get("files") or {}).items():
            if key not in ("background_image", "icon") or not isinstance(f, dict):
                continue
            try:
                os.makedirs(folder, exist_ok=True)
                name = os.path.basename(str(f.get("name") or key)) or key
                dst = os.path.join(folder, f"{t['id']}-{name}")
                with open(dst, "wb") as out:
                    out.write(base64.b64decode(f.get("data") or ""))
                t[key] = dst
            except (OSError, ValueError):
                log.exception("could not unpack %s from %s", key, path)
        self.add(t)
        return t

    def reset_builtin(self, tid: str):
        t = self.get(tid)
        if t and t.get("builtin") and tid in theme.PRESETS:
            fresh = _builtin(tid, t["name"])
            fresh["folder"] = t.get("folder", "")
            self.update_theme(fresh)
