"""A foreign workflow or unrelated passing CI must never sign a package cut."""

import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "arch_admission", ROOT / "packaging/arch/admit.py"
)
admission = importlib.util.module_from_spec(spec)
spec.loader.exec_module(admission)


class PublicationAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.repo = "nacre-desktop/nacre"
        self.run = dict(
            id=123,
            workflow_id=99,
            conclusion="success",
            head_branch="main",
            head_repository=dict(full_name=self.repo),
            event="workflow_run",
            head_sha="b" * 40,
        )
        # workflow_run can execute after main advanced; checkout source is older.
        self.receipt = dict(
            schema=1,
            sourceRevision="a" * 40,
            workflowRevision="b" * 40,
            runId="123",
            releasedOrient=True,
        )
        self.checks = [
            dict(
                head_sha="a" * 40,
                head_branch="main",
                head_repository=dict(full_name=self.repo),
                conclusion="success",
                event="push",
            )
        ]

    def test_admit_packaged_source_even_when_workflow_main_has_advanced(self):
        admission.validate_run(self.run, 99, self.repo)
        self.assertEqual(
            admission.validate_source(self.run, self.receipt, self.checks, self.repo),
            "a" * 40,
        )

    def test_foreign_fork_wrong_workflow_failed_build_and_pull_request_are_rejected(
        self,
    ):
        for field, value in (
            ("head_repository", dict(full_name="someone/fork")),
            ("workflow_id", 98),
            ("conclusion", "failure"),
            ("head_branch", "feature"),
            ("event", "pull_request"),
        ):
            run = {**self.run, field: value}
            with self.subTest(field=field), self.assertRaises(ValueError):
                admission.validate_run(run, 99, self.repo)

    def test_unrelated_ci_and_mismatched_receipts_are_rejected(self):
        for field, value in (
            ("sourceRevision", "c" * 40),
            ("workflowRevision", "c" * 40),
            ("runId", "124"),
            ("releasedOrient", False),
        ):
            receipt = {**self.receipt, field: value}
            with self.subTest(field=field), self.assertRaises(ValueError):
                admission.validate_source(self.run, receipt, self.checks, self.repo)
        foreign = copy.deepcopy(self.checks)
        foreign[0]["head_repository"]["full_name"] = "someone/fork"
        with self.assertRaises(ValueError):
            admission.validate_source(self.run, self.receipt, foreign, self.repo)
