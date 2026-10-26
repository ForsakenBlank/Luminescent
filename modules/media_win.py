"""
Media Controls on Windows: the media sessions Windows itself knows about (the same ones the volume
flyout shows: Spotify, browsers, the Media Player...), through the winrt packages
(winrt-Windows.Media.Control). Without them the play / next / previous buttons still work, as the
media keys, but there is no title or cover.

query() and control() block for a moment, so modules/media.py calls them on a worker thread.
"""
import os
import hashlib
import asyncio
import logging
import tempfile

log = logging.getLogger("media")
STATUS = {0: "Closed", 1: "Opened", 2: "Changing", 3: "Stopped", 4: "Playing", 5: "Paused"}
COVER_DIR = os.path.join(tempfile.gettempdir(), "luminescent-media")
_VK = {"play-pause": 0xB3, "next": 0xB0, "previous": 0xB1}


def available() -> bool:
    try:
        import winrt.windows.media.control  # noqa: F401
        return True
    except ImportError:
        return False


def _nice(app_id: str) -> str:
    """'Spotify.exe' -> 'Spotify', 'Microsoft.ZuneMusic_8wekyb3d8bbwe!App' -> 'ZuneMusic'."""
    name = app_id.split("!")[0].split("_")[0]
    if name.lower().endswith(".exe"):
        name = name[:-4]
    if "." in name:
        name = name.split(".")[-1]
    return name or app_id or "Player"


async def _cover(props) -> str:
    """The session's cover saved to a temp file (named after its contents), '' when there is none."""
    try:
        from winrt.windows.storage.streams import DataReader
        if props.thumbnail is None:
            return ""
        stream = await props.thumbnail.open_read_async()
        size = int(stream.size)
        if not size or size > 10_000_000:
            return ""
        reader = DataReader(stream.get_input_stream_at(0))
        await reader.load_async(size)
        data = bytearray(size)
        reader.read_bytes(data)
        os.makedirs(COVER_DIR, exist_ok=True)
        path = os.path.join(COVER_DIR, hashlib.sha1(data).hexdigest()[:16] + ".img")
        if not os.path.isfile(path):
            with open(path, "wb") as f:
                f.write(bytes(data))
        return "file://" + path.replace("\\", "/")
    except Exception:
        log.debug("cover failed", exc_info=True)
        return ""


async def _query() -> list[dict]:
    from winrt.windows.media.control import GlobalSystemMediaTransportControlsSessionManager as Manager
    mgr = await Manager.request_async()
    out = []
    for s in mgr.get_sessions():
        try:
            props = await s.try_get_media_properties_async()
            info = s.get_playback_info()
            tl = s.get_timeline_properties()
            pos = int(tl.position.total_seconds() * 1_000_000) if tl and tl.position else 0
            end = int(tl.end_time.total_seconds() * 1_000_000) if tl and tl.end_time else 0
            out.append({"player": _nice(s.source_app_user_model_id or ""), "id": s.source_app_user_model_id or "",
                        "status": STATUS.get(int(info.playback_status), "Stopped"),
                        "title": props.title or "", "artist": props.artist or "", "album": props.album_title or "",
                        "art": await _cover(props), "pos": pos, "len": end})
        except Exception:
            log.debug("a media session could not be read", exc_info=True)
    return out


def query() -> list[dict]:
    """Every media session, like media.parse() gives for playerctl ([] without the winrt packages)."""
    if not available():
        return []
    try:
        return asyncio.run(_query())
    except Exception as e:
        log.info("media sessions unavailable: %s", e)
        return []


async def _control(app_id: str, command: str) -> bool:
    from winrt.windows.media.control import GlobalSystemMediaTransportControlsSessionManager as Manager
    mgr = await Manager.request_async()
    sessions = [s for s in mgr.get_sessions() if s.source_app_user_model_id == app_id] or \
        [s for s in [mgr.get_current_session()] if s is not None]
    if not sessions:
        return False
    s = sessions[0]
    call = {"play-pause": s.try_toggle_play_pause_async, "next": s.try_skip_next_async,
            "previous": s.try_skip_previous_async}[command]
    return bool(await call())


def control(app_id: str, command: str):
    """Play/pause, next or previous on one player (or the media key when that can't be done)."""
    done = False
    if available():
        try:
            done = asyncio.run(_control(app_id, command))
        except Exception as e:
            log.info("media control failed: %s", e)
    if not done:
        import ctypes
        vk = _VK[command]
        ctypes.windll.user32.keybd_event(vk, 0, 0, 0)
        ctypes.windll.user32.keybd_event(vk, 0, 2, 0)      # KEYEVENTF_KEYUP
