#!/usr/bin/env python3
"""Fixture-only parity check for Brain discovery and public graph projection.

Usage: python brain/tests/verify_native.py --native target/debug/nacre-helper
Run from a disposable CI checkout with no real vault mounted or configured.
"""

from __future__ import annotations

import copy
import datetime as dt
import difflib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


TESTS = load_module(
    "brain_discovery_historical_fixtures", ROOT / "brain/tests/test_discovery.py"
)


def note(*args, **kwargs):
    return TESTS.note(*args, **kwargs)


def cases():
    result = []

    def add(name, notes, dense=None, prior=None, overrides=None, followup=None):
        result.append(
            (name, notes, dense or {}, prior or {}, overrides or {}, followup)
        )

    add(
        "historical-unknown-worlds-topics",
        [
            note(
                1,
                "A training result",
                "Grip practice",
                {"worlds": "Climbing", "topics": "Technique"},
            ),
            note(
                2,
                "Circuit result",
                "Measure supply ripple",
                {"worlds": "Electronics", "topics": "Measurements"},
            ),
        ],
    )
    add(
        "historical-manual-correction",
        [note(1, "A training outcome", "A grip technique", {"worlds": "Wrong label"})],
        overrides={"note:1": ["Climbing"]},
    )
    add(
        "historical-repeated-named-templates",
        [
            note(i, "GardenSense baseline", "Identical sensor baseline")
            for i in range(4)
        ],
    )
    promoted = [
        note(
            i,
            f"Climbing {i}",
            f"Grip result {i}",
            {"worlds": "Personal", "topics": "Climbing"},
            f"2026-10-0{1 + i % 2}",
        )
        for i in range(6)
    ]
    add(
        "historical-promoted-topic-prior-seed",
        promoted,
        followup=(
            promoted
            + [
                note(
                    20 + i,
                    f"A different interest {i}",
                    f"Distinct different interest {i}",
                    {"worlds": "Personal"},
                    "2026-10-03",
                )
                for i in range(25)
            ],
            {},
            None,
            None,
            "prior",
        ),
    )
    shared = note(
        1,
        "An interdisciplinary idea",
        "A wearable for training",
        {"worlds": "Climbing, Electronics"},
    )
    shared["confidence"] = "unverified"
    add("historical-shared-annotations", [shared])
    add("historical-single-mention", [note(1, "A telescope thought", "Try astronomy")])
    add(
        "historical-duplicate-templates",
        [
            note(i, "The same template", "Telescope astronomy observation")
            for i in range(4)
        ],
    )
    novel = [
        note(
            1,
            "GardenSense moisture readings",
            "GardenSense soil moisture irrigation sensors",
        ),
        note(
            2,
            "GardenSense watering experiment",
            "GardenSense soil moisture irrigation timing",
        ),
        note(
            3,
            "GardenSense sensor calibration",
            "GardenSense soil moisture irrigation calibration",
        ),
    ]
    add(
        "historical-novel-coherent",
        novel,
        followup=(list(reversed(novel)), {}, None, None, "prior"),
    )
    named = [
        note(
            0,
            "Sensor electronics",
            "Soil moisture irrigation sensors",
            {"worlds": "Electronics"},
        ),
        note(
            1,
            "GardenSense moisture readings",
            "GardenSense soil moisture irrigation sensors",
        ),
        note(
            2,
            "GardenSense watering experiment",
            "GardenSense soil moisture irrigation timing",
        ),
        note(
            3,
            "GardenSense sensor calibration",
            "GardenSense soil moisture irrigation calibration",
        ),
    ]
    add("historical-new-named-project-dense", named, {n["id"]: [1, 0] for n in named})
    add(
        "historical-annotation-beats-similarity",
        [
            note(1, "Acme camera work", "Camera firmware optics", {"worlds": "Acme"}),
            note(
                2,
                "A camera hobby",
                "Camera optics photographs",
                {"worlds": "Photography"},
            ),
        ],
    )
    add(
        "historical-body-namespace-prefix",
        [
            dict(
                note(1, "Ada", "Personal climbing education"),
                path="wiki/personal.md",
                meta={"entity": "world", "name": "Ada", "project": "personal"},
            ),
            dict(
                note(2, "Nacre", "Desktop settings"),
                path="wiki/os.md",
                meta={"entity": "world", "name": "Nacre", "project": "os"},
            ),
            note(3, "A desktop tweak", "Nacre panel configuration"),
        ],
    )
    independent = [
        note(
            i,
            f"Climbing {i}",
            f"Distinct grip result {i}",
            {"worlds": "Personal", "topics": "Climbing"},
            f"2026-10-0{1 + i % 2}",
        )
        for i in range(6)
    ]
    add(
        "historical-topic-promotes-independent-days",
        independent,
        followup=(
            [dict(n, date="2026-10-01") for n in independent],
            {},
            None,
            None,
            "no-prior",
        ),
    )

    add(
        "extra-curated-identity-collision",
        [
            dict(
                note(
                    61,
                    "Alpha",
                    "A curated entity",
                    {"entity": "world", "name": "Alpha"},
                ),
                path="wiki/alpha/Index.md",
            ),
            dict(
                note(
                    62,
                    "Beta",
                    "Another entity",
                    {"entity": "world", "name": "Beta", "project": ""},
                ),
                path="wiki/beta/Index.md",
            ),
            dict(
                note(
                    63,
                    "Gamma",
                    "An explicit entity",
                    {"entity": "world", "name": "Gamma", "project": "public.fixture"},
                ),
                path="wiki/gamma.md",
            ),
        ],
    )

    add(
        "extra-non-latin-subject-identities",
        [
            note(
                70,
                "A public item",
                "prism grating spectroscopy",
                {"worlds": "知识,研究"},
            )
        ],
    )

    # Additional public-only cases exercise curated links, explicit entity pages,
    # ordered annotations, merged topics, profile evidence and Unicode identity.
    curated = [
        dict(
            note(
                30,
                "Nacre",
                "Entity page",
                {
                    "entity": "world",
                    "name": "Nacre",
                    "project": "nacre",
                    "aliases": "Nacre OS",
                },
            ),
            path="wiki/nacre.md",
        ),
        dict(
            note(
                31,
                "Research topic",
                "Topic page",
                {"entity": "topic", "name": "Research", "parent": "Nacre"},
            ),
            path="wiki/research.md",
        ),
        dict(
            note(32, "Linked evidence", "A public project source"),
            references=["note:30"],
        ),
        note(33, "A Nacre release", "Release notes", {"worlds": "Nacre, Research"}),
    ]
    add("extra-curated-links-and-entities", curated)
    ordering = [
        note(40, "First", "One", {"worlds": "Électronique, Audio"}),
        note(41, "Second", "Two", {"worlds": "Audio, Électronique"}),
        note(42, "Third", "Three", {"worlds": "Audio, Électronique"}),
    ]
    add("extra-annotated-ordering-unicode", ordering)
    add("extra-annotated-ordering-unicode-reversed", list(reversed(ordering)))
    topics = [
        note(
            i,
            f"Signal processing {i}",
            f"Waveform measurement result {i}",
            {"worlds": "Audio", "topics": "DSP, d.s.p., Signal analysis"},
            f"2026-10-0{1 + i % 2}",
        )
        for i in range(1, 7)
    ]
    add("extra-topic-merge-and-promote", topics)
    add(
        "extra-prior-and-overrides",
        [note(50, "A revised item", "Audio calibration", {"worlds": "Old World"})],
        overrides={"note:50": ["Électronique", "Audio"]},
    )
    return result


def canonical(value):
    return json.dumps(
        value,
        default=lambda v: sorted(v) if isinstance(v, set) else str(v),
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    )


def native_call(
    binary: Path, tool: str, operation: str, request: dict, env: dict, timeout=30
):
    argv = [
        str(binary),
        tool,
        operation,
        json.dumps(request, ensure_ascii=False, separators=(",", ":")),
    ]
    completed = subprocess.run(
        argv, env=env, text=True, capture_output=True, timeout=timeout, check=False
    )
    if completed.returncode:
        raise RuntimeError(
            f"{' '.join(argv[:2])} failed ({completed.returncode}): {completed.stderr}{completed.stdout}"
        )
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(
            f"invalid JSON from {' '.join(argv[:2])}: {completed.stdout!r}"
        ) from error


def compare_discovery(binary: Path):
    module = load_module("brain_discovery_reference", ROOT / "brain/discovery.py")
    total = 0
    failures = []
    for name, notes, dense, prior, overrides, followup in cases():
        steps = [(notes, dense, prior, overrides, False)]
        if followup:
            next_notes, next_dense, next_overrides, _unused, prior_mode = followup
            steps.append(
                (
                    next_notes,
                    next_dense,
                    {},
                    next_overrides or {},
                    prior_mode == "prior",
                )
            )
        current_prior = prior
        for index, (
            step_notes,
            step_dense,
            step_prior,
            step_overrides,
            use_previous,
        ) in enumerate(steps):
            expected_notes = copy.deepcopy(step_notes)
            expected = module.organize(
                expected_notes,
                step_dense,
                current_prior if use_previous else step_prior,
                step_overrides,
            )
            request = {
                "args": [copy.deepcopy(step_notes)],
                "kwargs": {
                    "dense": step_dense,
                    "prior": current_prior if use_previous else step_prior,
                    "overrides": step_overrides,
                },
            }
            with tempfile.TemporaryDirectory(prefix="nacre-brain-discovery-") as home:
                env = os.environ.copy()
                env.update(
                    {
                        "HOME": home,
                        "NACRE_BRAIN": str(Path(home) / "fixture-vault"),
                        "NACRE_BRAIN_DIR": str(Path(home) / "fixture-vault"),
                        "NACRE_BRAIN_SEMANTICS": "0",
                    }
                )
                actual = native_call(
                    binary, "brain-discovery", "organize", request, env
                )
            if canonical(expected) != canonical(actual):
                left = json.dumps(
                    expected,
                    default=lambda v: sorted(v) if isinstance(v, set) else str(v),
                    ensure_ascii=False,
                    indent=2,
                    sort_keys=True,
                ).splitlines()
                right = json.dumps(
                    actual, ensure_ascii=False, indent=2, sort_keys=True
                ).splitlines()
                diff = "\n".join(
                    difflib.unified_diff(
                        left, right, fromfile="python", tofile="native", lineterm=""
                    )
                )
                failures.append(
                    f"discovery parity failed: {name} run {index + 1}\n{diff}"
                )
            current_prior = expected[1]
            total += 1
            print(f"CHECKED discovery {name} run {index + 1}")
    if failures:
        raise AssertionError("\n".join(failures))
    return total


GRAPH_FILES = {
    "inbox/anchor-a.md": "# A\nWorlds: First world\nprism grating spectroscopy\n",
    "inbox/anchor-b.md": "# B\nWorlds: Second world\nprism grating spectroscopy\n",
    "inbox/uncertain.md": "# Uncertain\nprism grating spectroscopy\n",
    "inbox/aged.md": "# A dated observation\nRecorded: {old_date}\nWorlds: History\nEvidence older than five half lives.\n",
    "inbox/unassigned.md": "# A telescope thought\nTry astronomy\n",
    "wiki/nacre.md": "# Nacre\nEntity: world\nName: Nacre\nProject: nacre\nAliases: Nacre OS\n\nA platform entity.\n",
    "wiki/research.md": "# Research\nEntity: topic\nName: Research\nParent: Nacre\n\nCurated topic page.\n",
    "projects/release.md": "# Nacre release notes\nRecorded: {today}T12:00:00+00:00\nConfidence: verified\nWorlds: Nacre, Research\nSource: public fixture\n\nSee [[nacre]] and [topic](../wiki/research.md).\n",
    "inbox/measurement.md": "# Signal measurement\nRecorded: {today}T12:00:00+00:00\nConfidence: reported\nWorlds: Audio\nTopics: Signal processing\nSource: public fixture\n\nWaveform measurement linked to [[release]].\n",
}


def populate_vault(vault: Path, today: str):
    for relative, body in GRAPH_FILES.items():
        path = vault / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            body.format(
                today=today,
                old_date=(
                    dt.date.fromisoformat(today) - dt.timedelta(days=105)
                ).isoformat(),
            ),
            encoding="utf-8",
        )


def graph_projection(value):
    nodes = copy.deepcopy(value["nodes"])
    links = copy.deepcopy(value["links"])
    # Fixed fixture dates make activity deterministic. Link iteration reflects
    # Python set ordering, so compare source/group edges without list order.
    links.sort(key=canonical)
    result = copy.deepcopy(value)
    result.pop("generated", None)
    result.update(nodes=nodes, links=links)
    return result


def compare_graph(binary: Path):
    today = dt.datetime.now(dt.timezone.utc).date().isoformat()
    with tempfile.TemporaryDirectory(prefix="nacre-brain-graph-") as temp:
        root = Path(temp)
        py_home = root / "python-home"
        native_home = root / "native-home"
        py_home.mkdir()
        native_home.mkdir()
        py_vault = py_home / "fixture-vault"
        native_vault = native_home / "fixture-vault"
        populate_vault(py_vault, today)
        populate_vault(native_vault, today)

        old_home = os.environ.get("HOME")
        old_dir = os.environ.get("NACRE_BRAIN_DIR")
        os.environ.update(
            {
                "HOME": str(py_home),
                "NACRE_BRAIN_DIR": str(py_vault),
                "NACRE_BRAIN_SEMANTICS": "0",
            }
        )
        try:
            control = load_module(
                "brain_control_graph_reference", ROOT / "brain/control.py"
            )
            python_graph = control.graph()
        finally:
            if old_home is None:
                os.environ.pop("HOME", None)
            else:
                os.environ["HOME"] = old_home
            if old_dir is None:
                os.environ.pop("NACRE_BRAIN_DIR", None)
            else:
                os.environ["NACRE_BRAIN_DIR"] = old_dir

        env = os.environ.copy()
        env.update(
            {
                "HOME": str(native_home),
                "NACRE_BRAIN_DIR": str(native_vault),
                "NACRE_BRAIN_SEMANTICS": "0",
            }
        )
        native_graph = native_call(binary, "brain-control", "graph", {}, env)
        expected = graph_projection(python_graph)
        actual = graph_projection(native_graph)
        if canonical(expected) != canonical(actual):
            left = json.dumps(
                expected,
                default=lambda v: sorted(v) if isinstance(v, set) else str(v),
                ensure_ascii=False,
                indent=2,
                sort_keys=True,
            ).splitlines()
            right = json.dumps(
                actual, ensure_ascii=False, indent=2, sort_keys=True
            ).splitlines()
            diff = "\n".join(
                difflib.unified_diff(
                    left,
                    right,
                    fromfile="python-graph",
                    tofile="native-graph",
                    lineterm="",
                )
            )
            raise AssertionError(f"public graph parity failed\n{diff}")
        print("PASS public fixture graph nodes/links")


def main():
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--native", type=Path, required=True, help="built nacre-helper executable"
    )
    parser.add_argument(
        "--no-graph", action="store_true", help="compare discovery only"
    )
    args = parser.parse_args()
    binary = args.native.resolve()
    if not binary.is_file():
        parser.error(f"native executable does not exist: {binary}")
    count = compare_discovery(binary)
    if not args.no_graph:
        compare_graph(binary)
    print(f"PASS {count} discovery parity runs")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
