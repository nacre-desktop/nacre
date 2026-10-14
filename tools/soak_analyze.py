#!/usr/bin/env python3
"""Print bounded per-process and action summaries from an existing soak capture."""

import argparse
import csv
import json
import math
from collections import Counter, defaultdict
from pathlib import Path

from soak import slope


def combined(row):
    """Resident plus swapped PSS; missing swap coverage must stay unknown."""
    if row.get("pss_mib") in (None, "") or row.get("swap_pss_mib") in (None, ""):
        return None
    return float(row["pss_mib"]) + float(row["swap_pss_mib"])


def correlation(pairs):
    if len(pairs) < 5:
        return None
    xs, ys = zip(*pairs, strict=True)
    xm, ym = sum(xs) / len(xs), sum(ys) / len(ys)
    xx = sum((x - xm) ** 2 for x in xs)
    yy = sum((y - ym) ** 2 for y in ys)
    return (
        sum((x - xm) * (y - ym) for x, y in pairs) / math.sqrt(xx * yy)
        if xx and yy
        else None
    )


def summarize(directory):
    report = json.loads((directory / "report.json").read_text())
    with (directory / "samples.csv").open() as stream:
        rows = list(csv.DictReader(stream))
    events = [
        json.loads(line)
        for line in (directory / "events.jsonl").read_text().splitlines()
    ]
    groups = defaultdict(list)
    for row in rows:
        if row["phase"] == "measured" and row["role"] != "total":
            groups[(row["role"], row["pid"], row["start"])].append(row)
    trends, brief = [], Counter()
    for (role, pid, start), samples in sorted(groups.items()):
        points = [
            (float(row["seconds"]), combined(row))
            for row in samples
            if combined(row) is not None
        ]
        fit = slope(points)
        if not fit:
            brief[role] += 1
            continue
        tail = slope([point for point in points if point[0] >= points[-1][0] - 3600])
        head = slope([point for point in points if point[0] <= points[0][0] + 3600])
        classification = "no sustained rise"
        if tail and tail["per_hour"] > 5 and tail["r_squared"] >= 0.6:
            classification = "continued growth; no observed plateau"
        elif fit["per_hour"] > 5 and fit["r_squared"] >= 0.6:
            classification = "earlier growth; tail does not establish continued rise"
        metrics = {}
        for key in ("pss_mib", "fds", "threads", "children"):
            metrics[key] = slope(
                [
                    (float(row["seconds"]), float(row[key]))
                    for row in samples
                    if row[key] not in (None, "")
                ]
            )
        trends.append(
            dict(
                role=role,
                pid=pid,
                start=start,
                combined=fit,
                first_hour=head,
                last_hour=tail,
                classification=classification,
                metrics=metrics,
            )
        )
    shell = next(
        (samples for identity, samples in groups.items() if identity[0] == "shell"), []
    )
    associations = defaultdict(list)
    for before, after in zip(shell, shell[1:], strict=False):
        a, b = combined(before), combined(after)
        if a is None or b is None:
            continue
        counts = Counter(
            event["action"]
            for event in events
            if event["kind"] == "action"
            and float(before["seconds"]) < event["seconds"] <= float(after["seconds"])
        )
        for name in report["actions"]:
            associations[name].append((counts[name], b - a))
    action_summary = {
        name: dict(intervals=len(pairs), count_vs_memory_delta_r=correlation(pairs))
        for name, pairs in associations.items()
    }
    notices = [event for event in events if event["kind"] == "notification_counts"]
    result = dict(
        backend=report["backend"],
        revision=report["source_revision"],
        elapsed_seconds=report["elapsed_seconds"],
        healthy=report["workload_healthy"],
        trends=trends,
        short_lifetimes=dict(brief),
        actions=action_summary,
        notification_peak={
            key: max((v.get(key, 0) for v in notices), default=0)
            for key in ("retained", "popups", "dismissedKeys")
        },
        notification_last={
            key: notices[-1].get(key) for key in ("retained", "popups", "dismissedKeys")
        }
        if notices
        else {},
    )
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directories", type=Path, nargs="+")
    parser.add_argument(
        "--save", type=Path, help="write summary JSON, never raw samples"
    )
    args = parser.parse_args()
    results = [summarize(path) for path in args.directories]
    for result in results:
        print(
            f"{result['backend']}: {result['elapsed_seconds'] / 60:.1f} elapsed min; healthy={result['healthy']}"
        )
        for trend in result["trends"]:
            fit, tail = trend["combined"], trend["last_hour"]
            tail_text = (
                f"{tail['per_hour']:+.1f}/h R2={tail['r_squared']:.3f}"
                if tail
                else "insufficient coverage"
            )
            print(
                f"  {trend['role']} pid={trend['pid']} n={fit['samples']} combined={fit['first']:.1f}->{fit['last']:.1f} MiB; "
                f"slope={fit['per_hour']:+.1f}/h R2={fit['r_squared']:.3f}; tail={tail_text}; {trend['classification']}"
            )
        print(
            f"  Short observed lifetimes (<5 valid samples): {result['short_lifetimes']}"
        )
        print(
            f"  Notification peaks: {result['notification_peak']}; last: {result['notification_last']}"
        )
        print(
            "  Action count vs next-sample memory delta (mixed workloads; correlation is not attribution): "
            + ", ".join(
                f"{name}={value['count_vs_memory_delta_r']:+.2f}"
                for name, value in result["actions"].items()
                if value["count_vs_memory_delta_r"] is not None
            )
        )
    if args.save:
        args.save.write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
