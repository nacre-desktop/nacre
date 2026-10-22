"""Fixture-only offline worker protocol/cache regression; no model downloads."""

import argparse
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    binary = parser.parse_args().binary.resolve()
    with tempfile.TemporaryDirectory(prefix="nacre-semantic-fixture-") as temporary:
        home = Path(temporary)
        vault = home / "vault"
        vault.mkdir()
        (vault / "fixture.md").write_text(
            "# Public waveform fixture\nWorlds: Audio\nRecorded: 2026-10-22\nA fixture only.\n"
        )
        python = home / ".local/share/nacre/brain-model/bin/python"
        python.parent.mkdir(parents=True)
        python.write_text(
            '#!/usr/bin/python3\nimport json,os,sys\nwith open(os.environ["FIXTURE_MODEL_LOG"],"a") as stream:\n    stream.write("started\\n")\nfor line in sys.stdin:\n    request=json.loads(line)\n    assert set(request)=={"texts"} and len(request["texts"])<=12\n    print(json.dumps({"vectors":[] if os.environ.get("FIXTURE_MODEL_BAD") else [[1.0,0.0] for text in request["texts"]]}),flush=True)\n'
        )
        python.chmod(0o700)
        log = home / "worker.log"
        env = {
            **os.environ,
            "HOME": str(home),
            "NACRE_BRAIN": str(vault),
            "NACRE_BRAIN_DIR": str(vault),
            "NACRE_BRAIN_SEMANTICS": "1",
            "FIXTURE_MODEL_LOG": str(log),
        }

        def graph(extra=None):
            return json.loads(
                subprocess.check_output(
                    [str(binary), "brain-control", "graph"],
                    env={**env, **(extra or {})},
                    timeout=35,
                )
            )

        first = graph()
        assert first["discovery"]["embedding"] == "local semantic model"
        cache_path = vault / ".brain-state/discovery/vectors.json"
        cache = json.loads(cache_path.read_text())
        assert len(cache) == 1
        assert next(iter(cache.values()))["vector"] == [1.0, 0.0]
        assert set(next(iter(cache.values()))) == {"vector", "revision"}
        assert cache_path.stat().st_mode & 0o777 == 0o600
        assert log.read_text().splitlines() == ["started"]
        graph()
        assert log.read_text().splitlines() == ["started"], (
            "unchanged revisions must reuse vectors"
        )
        with (vault / "fixture.md").open("a") as f:
            f.write("Changed public evidence.\n")
        graph()
        assert log.read_text().splitlines() == ["started", "started"]
        cache_path.unlink()
        fallback = graph({"FIXTURE_MODEL_BAD": "1"})
        assert fallback["discovery"]["embedding"] == "lexical fallback"
        started = time.monotonic()
        subprocess.run(
            [str(binary), "brain-control", "serve"],
            env={
                **env,
                "NACRE_BRAIN_PORT": "0",
                "NACRE_BRAIN_IDLE_SECONDS": "1",
                "NACRE_BRAIN_SEMANTICS": "0",
            },
            timeout=10,
            check=True,
        )
        assert 1 <= time.monotonic() - started < 10
        print("PASS native CLI idle exit on an ephemeral loopback port")
        print(
            "PASS semantic worker: bounded batches, revision cache reuse/invalidation, private vector-only state, malformed reply fallback"
        )


if __name__ == "__main__":
    main()
