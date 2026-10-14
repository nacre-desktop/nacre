"""Revision cache and bounded local worker; no document upload or inference network calls."""

import json
import nacre_state as _state
import os
import selectors
import subprocess
import threading
from pathlib import Path


class SemanticClient:
    def __init__(self, root):
        self.root = root
        self.process = None
        self.lock = threading.Lock()
        self.health = "lexical fallback"

    def vectors(self, documents, path):
        if os.environ.get("NACRE_BRAIN_SEMANTICS") == "0":
            return {}
        python = Path.home() / ".local/share/nacre/brain-model/bin/python"
        if not python.exists():
            return {}
        with self.lock:
            try:
                cache = _state.read_json_file(path) if path.exists() else {}
            except (OSError, ValueError):
                cache = {}
            missing = [
                n
                for n in documents
                if cache.get(n["id"], {}).get("revision") != n["revision"]
            ]
            if missing:
                try:
                    if self.process is None or self.process.poll() is not None:
                        self.process = subprocess.Popen(
                            [str(python), str(self.root / "semantic.py")],
                            stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL,
                            text=True,
                            bufsize=1,
                        )
                    for start in range(0, len(missing), 12):
                        batch = missing[start : start + 12]
                        self.process.stdin.write(
                            json.dumps({"texts": [n["featureText"] for n in batch]})
                            + "\n"
                        )
                        self.process.stdin.flush()
                        with selectors.DefaultSelector() as selector:
                            selector.register(self.process.stdout, selectors.EVENT_READ)
                            if not selector.select(25):
                                raise TimeoutError("Embedding timeout")
                        reply = json.loads(self.process.stdout.readline())
                        if len(reply.get("vectors", [])) != len(batch):
                            raise ValueError("Incomplete embeddings")
                        for n, vector in zip(batch, reply["vectors"], strict=True):
                            cache[n["id"]] = {
                                "revision": n["revision"],
                                "vector": vector,
                            }
                    # Drop vanished entries; cache only vectors/hashes, not document bodies.
                    cache = {
                        n["id"]: cache[n["id"]] for n in documents if n["id"] in cache
                    }
                    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                    tmp = path.with_suffix(".tmp")
                    tmp.write_text(json.dumps(cache))
                    tmp.chmod(0o600)
                    _state.write(path, cache)
                    tmp.unlink(missing_ok=True)
                except (
                    OSError,
                    ValueError,
                    KeyError,
                    TypeError,
                    subprocess.SubprocessError,
                ):
                    if self.process and self.process.poll() is None:
                        self.process.terminate()
                    self.process = None
                    self.health = "lexical fallback"
                    return {}
            self.health = "local semantic model"
            return {
                n["id"]: cache[n["id"]]["vector"] for n in documents if n["id"] in cache
            }
