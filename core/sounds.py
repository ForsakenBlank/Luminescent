"""
Sound playback for timers and reminders.

`play(kind, file)` where kind is one of the SOUND_CHOICES values in settings:
  none   - silence
  chime  - the freedesktop "complete" sound
  alarm  - the freedesktop "alarm-clock-elapsed" sound
  custom - `file`, played by pw-play / paplay (wav, ogg, flac...), else QtMultimedia (mp3, m4a...)
"""
import os
import shutil
import logging
import subprocess

log = logging.getLogger("sounds")

SOUND_DIRS = ["/usr/share/sounds/freedesktop/stereo", "/usr/share/sounds/ocean/stereo"]
SYSTEM = {"chime": ["complete", "message", "bell"], "alarm": ["alarm-clock-elapsed", "dialog-warning", "bell"]}
NATIVE_EXT = {".wav", ".ogg", ".oga", ".flac", ".opus"}

_player = None
_audio_out = None
_procs: list[subprocess.Popen] = []


def _system_file(kind: str) -> str:
    for name in SYSTEM.get(kind, []):
        for folder in SOUND_DIRS:
            for ext in (".oga", ".ogg", ".wav"):
                p = os.path.join(folder, name + ext)
                if os.path.isfile(p):
                    return p
    return ""


def _native_play(path: str, volume: float = 1.0) -> bool:
    """pw-play / paplay in the background. False if neither is installed."""
    vol = max(0.0, min(1.0, volume))
    if shutil.which("pw-play"):
        cmd = ["pw-play", "--volume", f"{vol:.2f}", path]
    elif shutil.which("paplay"):
        cmd = ["paplay", f"--volume={int(vol * 65536)}", path]
    else:
        return False
    try:
        _procs[:] = [p for p in _procs if p.poll() is None]
        _procs.append(subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                       start_new_session=True))
        return True
    except OSError:
        log.exception("could not start %s", cmd[0])
        return False


def _qt_play(path: str, volume: float = 1.0) -> bool:
    global _player, _audio_out
    try:
        from PyQt6.QtMultimedia import QMediaPlayer, QAudioOutput
        from PyQt6.QtCore import QUrl
    except Exception:
        return False
    try:
        if _player is None:
            _audio_out = QAudioOutput()
            _player = QMediaPlayer()
            _player.setAudioOutput(_audio_out)
        _audio_out.setVolume(max(0.0, min(1.0, volume)))
        _player.stop()
        _player.setSource(QUrl.fromLocalFile(path))
        _player.play()
        return True
    except Exception:
        log.exception("QtMultimedia playback failed")
        return False


def _beep():
    from PyQt6.QtWidgets import QApplication
    QApplication.beep()


def play(kind: str, file: str = "", volume: float = 1.0):
    kind = (kind or "none").lower()
    try:
        if kind == "none":
            return
        if kind in SYSTEM:
            path = _system_file(kind)
            if not (path and _native_play(path, volume)):
                _beep()
        elif kind == "custom":
            if not file or not os.path.exists(file):
                log.warning("custom sound file missing: %r, using chime", file)
                play("chime", volume=volume)
                return
            ext = os.path.splitext(file)[1].lower()
            if ext in NATIVE_EXT and _native_play(file, volume):
                return
            if not _qt_play(file, volume) and not _native_play(file, volume):
                _beep()
    except Exception:
        log.exception("could not play sound")


def stop():
    for p in _procs:
        if p.poll() is None:
            try:
                p.terminate()
            except OSError:
                pass
    _procs.clear()
    if _player is not None:
        try:
            _player.stop()
        except Exception:
            pass
