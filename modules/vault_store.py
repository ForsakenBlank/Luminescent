"""
Ghost Vault storage layer (no Qt in here so it can be tested on its own).

Layout of the vault folder
  .salt      random salt for PBKDF2
  .master    file key wrapped with the real password's derived key
  .dmaster   decoy file key wrapped with the decoy password's derived key (optional)
  .recovery  file key wrapped with the recovery key
  .meta      real index   (JSON, encrypted with the real file key)
  .dmeta     decoy index  (JSON, encrypted with the decoy file key)
  <hex>      encrypted file blobs, nonce + AES-GCM ciphertext

Real and decoy vaults are completely independent: the old version shared one
index file between them, so opening the decoy and adding a file silently
destroyed the real index.

Index format: {"tree": {name: "<blob hex>" | {subfolder...}}, "perm": {"a/b/name": "C:/decrypted/copy"}}

Pre-rewrite vaults (.hash/.dhash + password-derived file key) are migrated in
place the first time they are unlocked.
"""
import os
import json
import time
import secrets
import hashlib
import shutil
import threading
from pathlib import Path

from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
from cryptography.hazmat.primitives import hashes

PBKDF2_ITERS = 480_000


class VaultError(Exception):
    pass


# ── crypto primitives ────────────────────────────────────────────────────────
def derive_key(password: str, salt: bytes) -> bytes:
    kdf = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=salt, iterations=PBKDF2_ITERS)
    return kdf.derive(password.encode("utf-8"))


def _seal(key: bytes, data: bytes, aad: bytes = b"") -> bytes:
    nonce = secrets.token_bytes(12)
    return nonce + AESGCM(key).encrypt(nonce, data, aad)


def _write_atomic(path: Path, data: bytes):
    """Temp file + fsync + rename: a crash or power cut never leaves a half-written key, index or blob."""
    tmp = path.with_name(f"{path.name}.{secrets.token_hex(4)}.tmp")    # unique, so two writers never share one
    try:
        with open(tmp, "wb") as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
    except BaseException:
        try:
            tmp.unlink()
        except OSError:
            pass
        raise


def _open(key: bytes, blob: bytes, aad: bytes = b"") -> bytes:
    if len(blob) < 28:
        raise VaultError("corrupt data")
    return AESGCM(key).decrypt(blob[:12], blob[12:], aad)


def recovery_key_to_bytes(text: str, salt: bytes) -> bytes:
    cleaned = "".join(c for c in text.strip().upper() if c.isalnum())
    return hashlib.sha256(salt + cleaned.encode("ascii")).digest()


def format_recovery_key(raw_hex: str) -> str:
    raw_hex = raw_hex.upper()
    return "-".join(raw_hex[i:i + 4] for i in range(0, len(raw_hex), 4))


def secure_delete(path: Path, passes: int = 3):
    try:
        size = path.stat().st_size
        if size:
            with open(path, "r+b") as f:
                for _ in range(max(1, passes)):
                    f.seek(0)
                    remaining = size
                    while remaining > 0:
                        chunk = min(remaining, 1 << 20)
                        f.write(secrets.token_bytes(chunk))
                        remaining -= chunk
                    f.flush()
                    os.fsync(f.fileno())
        path.unlink()
    except FileNotFoundError:
        pass
    except Exception:
        try:
            path.unlink()
        except Exception:
            pass


def unique_path(path: Path) -> Path:
    if not path.exists():
        return path
    i = 1
    while True:
        cand = path.with_name(f"{path.stem}_{i}{path.suffix}")
        if not cand.exists():
            return cand
        i += 1


def unique_name(name: str, existing) -> str:
    if name not in existing:
        return name
    stem, ext = os.path.splitext(name)
    i = 1
    while f"{stem}_{i}{ext}" in existing:
        i += 1
    return f"{stem}_{i}{ext}"


# ── the store ────────────────────────────────────────────────────────────────
class VaultStore:
    def __init__(self, folder: str, decrypt_out: str, delete_passes: int = 3):
        self.folder = Path(folder)
        self.out = Path(decrypt_out)
        self.passes = delete_passes
        self.key: bytes | None = None
        self.mode: str | None = None          # "real" | "decoy"
        self.index: dict = {"tree": {}, "perm": {}}
        self.temp_files: list[Path] = []
        self.pending_recovery_key: str | None = None   # set when migration created one
        self._index_lock = threading.Lock()            # the UI thread and a worker thread both save the index

    # paths
    @property
    def salt_file(self): return self.folder / ".salt"
    @property
    def master_file(self): return self.folder / ".master"
    @property
    def dmaster_file(self): return self.folder / ".dmaster"
    @property
    def recovery_file(self): return self.folder / ".recovery"
    @property
    def index_file(self): return self.folder / (".dmeta" if self.mode == "decoy" else ".meta")

    # ── state queries ──────────────────────────────────────────────────────
    def check_writable(self) -> str:
        """Return '' if the vault folder can be created and written to, else an error."""
        try:
            self.folder.mkdir(parents=True, exist_ok=True)
            t = self.folder / ".writetest"
            t.write_bytes(b"ok")
            t.unlink()
            return ""
        except Exception as e:
            return str(e)

    def exists(self) -> bool:
        return self.master_file.exists() or (self.folder / ".hash").exists()

    def is_old_format(self) -> bool:
        return (self.folder / ".hash").exists() and not self.master_file.exists()

    def has_decoy(self) -> bool:
        return self.dmaster_file.exists() or (self.folder / ".dhash").exists()

    def is_unlocked(self) -> bool:
        return self.key is not None

    # ── creation / unlocking ───────────────────────────────────────────────
    def create(self, password: str, decoy_password: str | None = None) -> str:
        """Create a brand-new vault. Returns the recovery key to show the user."""
        self.folder.mkdir(parents=True, exist_ok=True)
        self.out.mkdir(parents=True, exist_ok=True)
        salt = secrets.token_bytes(32)
        _write_atomic(self.salt_file, salt)
        file_key = secrets.token_bytes(32)
        _write_atomic(self.master_file, _seal(derive_key(password, salt), file_key, b"master"))
        rec = self._write_recovery(file_key, salt)
        self.key, self.mode = file_key, "real"
        self.index = {"tree": {}, "perm": {}}
        self.save_index()
        if decoy_password:
            self.set_decoy_password(decoy_password)
        return rec

    def _write_recovery(self, file_key: bytes, salt: bytes) -> str:
        raw = secrets.token_hex(16)                      # 32 hex chars, 128 bits
        text = format_recovery_key(raw)
        _write_atomic(self.recovery_file, _seal(recovery_key_to_bytes(text, salt), file_key, b"recovery"))
        return text

    def unlock(self, password: str) -> str | None:
        """Try a password. Returns 'real', 'decoy' or None. Migrates old vaults on success."""
        if not self.salt_file.exists():
            return None
        salt = self.salt_file.read_bytes()
        pw_key = derive_key(password, salt)

        if self.master_file.exists():
            try:
                file_key = _open(pw_key, self.master_file.read_bytes(), b"master")
                self._set_unlocked(file_key, "real")
                return "real"
            except Exception:
                pass
            if self.dmaster_file.exists():
                try:
                    file_key = _open(pw_key, self.dmaster_file.read_bytes(), b"dmaster")
                    self._set_unlocked(file_key, "decoy")
                    return "decoy"
                except Exception:
                    pass

        # old-format fallbacks (sha256(salt+pw) hashes, file key == password-derived key)
        old_hash = hashlib.sha256(salt + password.encode("utf-8")).hexdigest()
        hash_file, dhash_file = self.folder / ".hash", self.folder / ".dhash"
        if hash_file.exists() and secrets.compare_digest(old_hash, hash_file.read_text().strip()):
            _write_atomic(self.master_file, _seal(pw_key, pw_key, b"master"))
            self.pending_recovery_key = self._write_recovery(pw_key, salt)
            hash_file.unlink()
            self._set_unlocked(pw_key, "real")
            return "real"
        if dhash_file.exists() and secrets.compare_digest(old_hash, dhash_file.read_text().strip()):
            dkey = secrets.token_bytes(32)
            _write_atomic(self.dmaster_file, _seal(pw_key, dkey, b"dmaster"))
            dhash_file.unlink()
            self._set_unlocked(dkey, "decoy")
            return "decoy"
        return None

    def unlock_with_recovery(self, recovery_key: str) -> bool:
        if not (self.recovery_file.exists() and self.salt_file.exists()):
            return False
        salt = self.salt_file.read_bytes()
        try:
            file_key = _open(recovery_key_to_bytes(recovery_key, salt), self.recovery_file.read_bytes(), b"recovery")
        except Exception:
            return False
        self._set_unlocked(file_key, "real")
        return True

    def _set_unlocked(self, key: bytes, mode: str):
        self.key, self.mode = key, mode
        self.index = {"tree": {}, "perm": {}}
        resave = False
        if self.index_file.exists():
            raw = self.index_file.read_bytes()
            data = None
            try:
                data = json.loads(_open(key, raw, b"index"))
            except Exception:
                try:
                    data = json.loads(_open(key, raw))      # pre-rewrite format used no AAD
                    resave = True
                except Exception:
                    data = None                            # unreadable (e.g. decoy overwrote it): start empty,
                    keep = self.index_file.with_name(f"{self.index_file.name}.unreadable-{int(time.time())}")
                    try:                                   # but keep the old file instead of overwriting it
                        os.replace(self.index_file, keep)
                    except OSError:
                        pass
            if isinstance(data, dict):
                if "tree" in data:
                    self.index = {"tree": data.get("tree", {}), "perm": data.get("perm", {})}
                else:  # old layout: {"files": {...}, "decoy_files": {...}, "perm_decrypted": {...}}
                    self.index = {"tree": data.get("files", {}), "perm": data.get("perm_decrypted", {})}
                    resave = True
        if resave:
            self.save_index()

    def lock(self):
        self.cleanup_temp()
        self.key, self.mode = None, None
        self.index = {"tree": {}, "perm": {}}

    # ── passwords ──────────────────────────────────────────────────────────
    def set_password(self, new_password: str):
        if self.mode != "real":
            raise VaultError("unlock the real vault first")
        salt = self.salt_file.read_bytes()
        _write_atomic(self.master_file, _seal(derive_key(new_password, salt), self.key, b"master"))

    def set_decoy_password(self, new_password: str | None):
        """Set/replace the decoy password. None removes the decoy. Existing decoy contents are kept if possible."""
        if self.mode != "real":
            raise VaultError("unlock the real vault first")
        if not new_password:
            for f in (self.dmaster_file, self.folder / ".dmeta"):
                if f.exists():
                    f.unlink()
            return
        salt = self.salt_file.read_bytes()
        dkey = secrets.token_bytes(32)
        dmeta = self.folder / ".dmeta"
        if dmeta.exists():
            dmeta.unlink()          # old decoy key is gone; its index cannot be read any more
        _write_atomic(self.dmaster_file, _seal(derive_key(new_password, salt), dkey, b"dmaster"))
        _write_atomic(dmeta, _seal(dkey, json.dumps({"tree": {}, "perm": {}}).encode(), b"index"))

    def regenerate_recovery_key(self) -> str:
        if self.mode != "real":
            raise VaultError("unlock the real vault first")
        return self._write_recovery(self.key, self.salt_file.read_bytes())

    # ── index ──────────────────────────────────────────────────────────────
    def save_index(self):
        with self._index_lock:
            _write_atomic(self.index_file, _seal(self.key, json.dumps(self.index).encode("utf-8"), b"index"))

    def node(self, path: list[str]) -> dict:
        n = self.index["tree"]
        for p in path:
            n = n[p]
            if not isinstance(n, dict):
                raise VaultError(f"{p} is not a folder")
        return n

    @staticmethod
    def perm_key(path: list[str], name: str) -> str:
        return "/".join(path + [name])

    def perm_copy(self, path: list[str], name: str) -> Path | None:
        p = self.index["perm"].get(self.perm_key(path, name))
        return Path(p) if p and Path(p).exists() else None

    # ── file operations ────────────────────────────────────────────────────
    def add_files(self, sources: list[str], path: list[str], move: bool, progress=None) -> list[str]:
        node = self.node(path)
        added = []
        for i, s in enumerate(sources):
            src = Path(s)
            if src.is_dir():
                sub = unique_name(src.name, node)
                node[sub] = {}
                self.add_files([str(c) for c in src.iterdir()], path + [sub], move)
                if move:
                    shutil.rmtree(src, ignore_errors=True)
                added.append(sub)
            elif src.is_file():
                name = unique_name(src.name, node)
                blob = secrets.token_hex(16)
                _write_atomic(self.folder / blob, _seal(self.key, src.read_bytes()))
                node[name] = blob
                added.append(name)
                if move:
                    self.save_index()          # record the blob before the only other copy is destroyed
                    secure_delete(src, self.passes)
            if progress:
                progress(int((i + 1) / max(1, len(sources)) * 100))
        self.save_index()
        return added

    def decrypt_file(self, path: list[str], name: str, temp: bool, out_dir: Path | None = None) -> Path:
        node = self.node(path)
        blob = node.get(name)
        if not isinstance(blob, str):
            raise VaultError(f"{name} is not a file")
        existing = self.perm_copy(path, name)
        if existing:
            return existing
        out_dir = out_dir or self.out
        out_dir.mkdir(parents=True, exist_ok=True)
        dst = unique_path(out_dir / name)
        dst.write_bytes(_open(self.key, (self.folder / blob).read_bytes()))
        if temp:
            self.temp_files.append(dst)
        else:
            self.index["perm"][self.perm_key(path, name)] = str(dst)
            self.save_index()
        return dst

    def decrypt_folder(self, path: list[str], name: str, temp: bool, progress=None):
        folder = self.node(path + [name])
        out = self.out / name
        items = list(folder.items())
        for i, (n, v) in enumerate(items):
            if isinstance(v, dict):
                self.decrypt_folder(path + [name], n, temp)
            else:
                self.decrypt_file(path + [name], n, temp, out_dir=out)
            if progress:
                progress(int((i + 1) / max(1, len(items)) * 100))

    def reencrypt(self, path: list[str], name: str):
        """Take the permanently decrypted copy back into the vault and securely delete it."""
        key = self.perm_key(path, name)
        p = self.index["perm"].pop(key, None)
        if p and Path(p).exists():
            node = self.node(path)
            blob = node.get(name)
            if not isinstance(blob, str):
                blob = secrets.token_hex(16)
                node[name] = blob
            _write_atomic(self.folder / blob, _seal(self.key, Path(p).read_bytes()))
            secure_delete(Path(p), self.passes)
        self.save_index()

    def _delete_tree(self, val, prefix: str):
        if isinstance(val, dict):
            for k, v in val.items():
                self._delete_tree(v, f"{prefix}/{k}" if prefix else k)
        else:
            secure_delete(self.folder / val, self.passes)
        p = self.index["perm"].pop(prefix, None)
        if p and Path(p).exists():
            secure_delete(Path(p), self.passes)

    def delete(self, path: list[str], names: list[str], progress=None):
        node = self.node(path)
        for i, n in enumerate(names):
            val = node.pop(n, None)
            if val is not None:
                self._delete_tree(val, self.perm_key(path, n))
            if progress:
                progress(int((i + 1) / max(1, len(names)) * 100))
        self.save_index()

    def unpack_folder(self, path: list[str], name: str):
        """Move a folder's contents up one level and remove the folder."""
        node = self.node(path)
        folder = node.get(name)
        if not isinstance(folder, dict):
            return
        for k, v in list(folder.items()):
            new = unique_name(k, node)
            node[new] = v
            old_key, new_key = self.perm_key(path + [name], k), self.perm_key(path, new)
            if old_key in self.index["perm"]:
                self.index["perm"][new_key] = self.index["perm"].pop(old_key)
        node.pop(name, None)
        self.save_index()

    def new_folder(self, path: list[str], name: str) -> str:
        node = self.node(path)
        name = unique_name(name, node)
        node[name] = {}
        self.save_index()
        return name

    def rename(self, path: list[str], name: str, new_name: str):
        node = self.node(path)
        if name not in node or not new_name or new_name == name:
            return
        new_name = unique_name(new_name, node)
        node[new_name] = node.pop(name)
        self._rekey_perm(self.perm_key(path, name), self.perm_key(path, new_name))
        self.save_index()

    def move(self, src_path: list[str], name: str, dst_path: list[str]):
        src, dst = self.node(src_path), self.node(dst_path)
        if name not in src:
            return
        if src_path == dst_path:
            return
        # don't move a folder into itself
        if dst_path[:len(src_path) + 1] == src_path + [name]:
            raise VaultError("cannot move a folder into itself")
        new_name = unique_name(name, dst)
        dst[new_name] = src.pop(name)
        self._rekey_perm(self.perm_key(src_path, name), self.perm_key(dst_path, new_name))
        self.save_index()

    def _rekey_perm(self, old_prefix: str, new_prefix: str):
        perm = self.index["perm"]
        for k in list(perm):
            if k == old_prefix or k.startswith(old_prefix + "/"):
                perm[new_prefix + k[len(old_prefix):]] = perm.pop(k)

    # ── temp files ─────────────────────────────────────────────────────────
    def cleanup_temp(self):
        for p in self.temp_files:
            if p.exists():
                secure_delete(p, self.passes)
        self.temp_files = []
