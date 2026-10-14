#!/usr/bin/env python3
"""Admit only a successful owner/main package build of source with passing CI."""

import json
import os
from pathlib import Path
import re
import subprocess


def validate_run(run, workflow_id, repository):
    if not (
        run["workflow_id"] == workflow_id
        and run["conclusion"] == "success"
        and run["head_branch"] == "main"
        and run["head_repository"]["full_name"] == repository
        and run["event"] in ("workflow_dispatch", "workflow_run")
        and re.fullmatch(r"[0-9a-f]{40}", run["head_sha"])
    ):
        raise ValueError("Only successful owner/main package builds may publish")


def validate_source(run, receipt, source_runs, repository):
    revision = receipt["sourceRevision"]
    if not (
        receipt["schema"] == 1
        and receipt["workflowRevision"] == run["head_sha"]
        and receipt["runId"] == str(run["id"])
        and receipt["releasedOrient"] is True
        and re.fullmatch(r"[0-9a-f]{40}", revision)
    ):
        raise ValueError("Package build receipt and workflow identity disagree")
    if not any(
        item["head_sha"] == revision
        and item["head_branch"] == "main"
        and item["head_repository"]["full_name"] == repository
        and item["conclusion"] == "success"
        and item["event"] == "push"
        for item in source_runs
    ):
        raise ValueError("Source CI must pass for the exact packaged source")
    return revision


def main():
    def api(path):
        return json.loads(subprocess.check_output(["gh", "api", path], text=True))

    repository = os.environ["GH_REPO"]
    run_id = os.environ["CANDIDATE_RUN"]
    if not run_id.isdecimal():
        raise ValueError("Run ID must be numeric")
    run = api(f"repos/{repository}/actions/runs/{run_id}")
    workflow = api(f"repos/{repository}/actions/workflows/arch-packages.yml")
    validate_run(run, workflow["id"], repository)
    artifacts = api(f"repos/{repository}/actions/runs/{run_id}/artifacts")["artifacts"]
    if not any(
        item["name"] == "pinned-arch-packages" and not item["expired"]
        for item in artifacts
    ):
        with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
            stream.write("eligible=false\n")
        print("No compatible package candidate in this run.")
        return
    subprocess.run(
        [
            "gh",
            "run",
            "download",
            run_id,
            "--repo",
            repository,
            "--name",
            "pinned-arch-packages",
            "--dir",
            "candidate",
        ],
        check=True,
    )
    eligible = any(Path("candidate/nacre").glob("nacre-[0-9]*.pkg.tar.zst"))
    revision = ""
    if eligible:
        receipt = json.loads(Path("candidate/build-receipt.json").read_text())
        source = receipt["sourceRevision"]
        if not re.fullmatch(r"[0-9a-f]{40}", source):
            raise ValueError("Invalid packaged revision")
        checks = api(
            f"repos/{repository}/actions/workflows/check.yml/runs?head_sha={source}&event=push&status=success"
        )["workflow_runs"]
        revision = validate_source(run, receipt, checks, repository)
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", revision, run["head_sha"]],
            check=True,
        )
    with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
        stream.write(
            f"eligible={str(eligible).lower()}\nrevision={revision}\nrun_id={run_id}\n"
        )
    if not eligible:
        print("Companion-only build: no Core repository promotion.")


if __name__ == "__main__":
    main()
