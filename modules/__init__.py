"""
Module registry. Each entry is imported lazily so that one module with a
missing dependency (e.g. Pillow) does not take the whole launcher down.
"""
import importlib
import logging

log = logging.getLogger("modules")

# (settings section key, "module_path:ClassName")
REGISTRY = [
    ("snippets", "modules.snippets:SnippetsModule"),
    ("notes", "modules.notes:NotesModule"),
    ("doodle", "modules.doodle:DoodleModule"),
    ("timer", "modules.timer:TimerModule"),
    ("vault", "modules.vault:VaultModule"),
    ("metadata", "modules.metadata:MetadataModule"),
    ("reminders", "modules.reminders:RemindersModule"),
    ("hearing", "modules.hearing:HearingModule"),
    ("calculator", "modules.calculator:CalculatorModule"),
    ("colorpicker", "modules.colorpicker:ColorPickerModule"),
    ("textgrab", "modules.textgrab:TextGrabModule"),
    ("promptlog", "modules.promptlog:PromptLogModule"),
    ("keybinds", "modules.keybinds:KeybindsModule"),
    ("snapper", "modules.snapper:SnapperModule"),
    ("ruler", "modules.ruler:RulerModule"),
    ("media", "modules.media:MediaModule"),
    ("qr", "modules.qr:QrModule"),
    ("sysmon", "modules.sysmon:SysmonModule"),
    ("atlas_sync", "modules.atlas_sync:AtlasSyncModule"),
]
# The File Launcher was retired on 2026-10-25: its shortcuts moved into the Dashboard (ui/dashboard.py).


def load_all():
    """Returns ({key: ModuleClass}, {key: error_string}) for the whole registry."""
    classes, errors = {}, {}
    for key, spec in REGISTRY:
        mod_path, cls_name = spec.split(":")
        try:
            mod = importlib.import_module(mod_path)
            classes[key] = getattr(mod, cls_name)
        except Exception as e:
            log.exception("could not import module %s", key)
            errors[key] = f"{type(e).__name__}: {e}"
    return classes, errors
