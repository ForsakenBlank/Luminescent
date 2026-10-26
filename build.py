"""
Build a standalone Luminescent binary with PyInstaller.

    python build.py            folder build: dist/Luminescent/Luminescent(.exe) (recommended: starts fastest)
    python build.py --onefile  a single dist/Luminescent(.exe) (unpacks to a temp folder on every start)
    python build.py --test     build, then run the binary's self-test (--smoke) in a throw-away data folder
    python build.py --release  a clean build for downloading: it never copies anyone's setup over

Works on Linux and Windows (build on the system you want the binary for).

The binary keeps its settings and data in ~/.local/share/Luminescent (%LOCALAPPDATA%\\Luminescent on
Windows). Unless --release is given, the first time it runs it copies settings.json and data/ from this
folder, so nothing is lost. Running from source keeps using this folder, so the two do not share changes
made afterwards.

Needs:  pip install pyinstaller   (plus requirements.txt)
"""
import os
import sys
import json
import shutil
import tempfile
import subprocess
from datetime import datetime

HERE = os.path.dirname(os.path.abspath(__file__))
ASSETS = os.path.join(HERE, "assets")


def make_icon() -> str:
    """Render the star with Qt and pack it into a multi-size .ico with Pillow."""
    from PyQt6.QtWidgets import QApplication
    from PIL import Image
    app = QApplication.instance() or QApplication(sys.argv[:1])
    sys.path.insert(0, HERE)
    from Launcher import star_pixmap
    os.makedirs(ASSETS, exist_ok=True)
    png = os.path.join(ASSETS, "luminescent-256.png")
    star_pixmap(256, light=False).save(png, "PNG")
    ico = os.path.join(ASSETS, "luminescent.ico")
    Image.open(png).save(ico, sizes=[(16, 16), (20, 20), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    del app
    return ico


def build(onefile: bool, release: bool = False) -> str:
    ico = make_icon()
    info = os.path.join(ASSETS, "build_info.json")
    with open(info, "w", encoding="utf-8") as f:
        json.dump({"source_dir": "" if release else HERE, "built": datetime.now().isoformat(timespec="seconds")}, f)
    win = sys.platform == "win32"
    platform_args = (["--collect-submodules", "winrt", "--collect-submodules", "pycaw",
                      "--hidden-import", "comtypes.stream", "--hidden-import", "keyboard"]
                     if win else ["--collect-submodules", "pulsectl"])
    args = [
        sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean", "--windowed",
        "--name", "Luminescent", "--icon", ico,
        "--distpath", os.path.join(HERE, "dist"), "--workpath", os.path.join(HERE, "build"),
        "--specpath", os.path.join(HERE, "build"),
        "--add-data", f"{info}{os.pathsep}.",
        "--add-data", f"{os.path.join(ASSETS, 'fonts')}{os.pathsep}assets/fonts",
        "--add-data", f"{os.path.join(ASSETS, 'luminescent-256.png')}{os.pathsep}assets",
        "--add-data", f"{os.path.join(ASSETS, 'luminescent-mark.png')}{os.pathsep}assets",
        # modules are imported by name at run time (modules/__init__.py), so list them
        "--collect-submodules", "modules", "--collect-submodules", "ui", "--hidden-import", "settings_ui",
        *platform_args,
        # opencv only makes video thumbnails in Snippets and adds ~100 MB: left out of the build
        "--exclude-module", "cv2", "--exclude-module", "tkinter", "--exclude-module", "matplotlib",
        "--exclude-module", "scipy",
        "--onefile" if onefile else "--onedir",
        os.path.join(HERE, "Launcher.py"),
    ]
    print(" ".join(args))
    subprocess.check_call(args, cwd=HERE)
    name = "Luminescent.exe" if win else "Luminescent"
    exe = os.path.join(HERE, "dist", name if onefile else os.path.join("Luminescent", name))
    print("\nbuilt", exe)
    return exe


def smoke(exe: str) -> int:
    home = tempfile.mkdtemp(prefix="lum-smoke-")
    if os.path.isfile(os.path.join(HERE, "settings.json")):
        shutil.copy2(os.path.join(HERE, "settings.json"), os.path.join(home, "settings.json"))
    env = dict(os.environ, LUMINESCENT_HOME=home, LUMINESCENT_NO_HOOK="1", QT_QPA_PLATFORM="offscreen")
    try:
        code = subprocess.call([exe, "--smoke"], env=env, timeout=120)
        log = os.path.join(home, "data", "launcher.log")
        if os.path.isfile(log):
            with open(log, encoding="utf-8", errors="replace") as f:
                lines = [l for l in f if "SMOKE" in l or "ERROR" in l or "Traceback" in l or "could not" in l]
            print("".join(lines[-20:]))
        print("smoke test exit code:", code)
        return code
    finally:
        shutil.rmtree(home, ignore_errors=True)


if __name__ == "__main__":
    exe = build("--onefile" in sys.argv, "--release" in sys.argv)
    if "--test" in sys.argv:
        sys.exit(smoke(exe))
