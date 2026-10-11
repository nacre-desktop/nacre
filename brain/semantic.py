# Offline/telemetry policy must be set before importing the model backend.
#!/usr/bin/env python3
"""Private offline CPU embeddings. Documents enter only over stdin, never arguments/logs."""

import json
import os
import sys
from pathlib import Path

MODEL = "BAAI/bge-small-en-v1.5"
CACHE = Path.home() / ".local/share/nacre/brain-model/models"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
if "--download" not in sys.argv:
    os.environ["HF_HUB_OFFLINE"] = "1"
from fastembed import TextEmbedding  # noqa: E402

model = TextEmbedding(
    model_name=MODEL,
    cache_dir=str(CACHE),
    threads=2,
    local_files_only="--download" not in sys.argv,
)
if "--download" in sys.argv:
    print(json.dumps({"model": MODEL, "ready": True}))
    sys.exit()
for line in sys.stdin:
    try:
        request = json.loads(line)
        values = list(model.embed(request["texts"], batch_size=16))
        print(json.dumps({"vectors": [v.tolist() for v in values]}), flush=True)
    except Exception:  # noqa: BLE001 — Optional model backend failures must not kill the line protocol.
        print(json.dumps({"error": "Local embeddings unavailable"}), flush=True)
