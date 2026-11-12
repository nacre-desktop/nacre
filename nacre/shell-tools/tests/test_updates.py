import importlib.util
from pathlib import Path
import subprocess
import unittest
import os
import shutil
import tempfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "updates", Path(__file__).resolve().parents[1] / "updates.py"
)
updates = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updates)


class UpdateTests(unittest.TestCase):
    def check(self, helper_result):
        results = [
            subprocess.CompletedProcess(["checkupdates"], 2, "", ""),
            helper_result,
        ]
        with (
            patch.object(updates.shutil, "which", side_effect=lambda name: name),
            patch.object(updates, "needs_host_runtime", return_value=False),
            patch.object(updates.subprocess, "run", side_effect=results),
        ):
            return updates.count_updates()

    def test_empty_aur_result_refreshes_to_zero(self):
        result = self.check(subprocess.CompletedProcess(["paru", "-Qua"], 1, "", ""))
        self.assertEqual(result["count"], 0)
        self.assertEqual(result["text"], "0")

    def test_aur_update_is_counted(self):
        result = self.check(
            subprocess.CompletedProcess(["paru", "-Qua"], 0, "chrome 1 -> 2\n", "")
        )
        self.assertEqual(result["count"], 1)

    def test_failed_aur_query_does_not_report_zero(self):
        with self.assertRaises(RuntimeError):
            self.check(
                subprocess.CompletedProcess(
                    ["paru", "-Qua"], 1, "", "error: network failure"
                )
            )


class UpdateFailureTests(unittest.TestCase):
    def test_failure_trap_reports_step_exit_and_keeps_skipreview(self):
        import os
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root.chmod(0o777)
            for name, body in {
                "gum": "exit 0",
                "python3": '[[ "$*" == *--needs-host ]] && exit 1; exit 0',
                "paru": 'printf "%s\\n" "$*"; exit 7',
                "pacman": '[[ "$1" == -Qq ]] && echo nacre || echo "nacre 1"',
                "nacre": 'echo "$*" >> "$CALL_LOG"',
                "flatpak": "exit 0",
            }.items():
                executable = root / name
                executable.write_text("#!/usr/bin/env bash\n" + body + "\n")
                executable.chmod(0o755)
            script = root / "updates.sh"
            shutil.copy2(Path(__file__).parents[1] / "updates.sh", script)
            script.chmod(0o644)
            command = ["bash", str(script)]
            if os.geteuid() == 0 and shutil.which("setpriv"):
                command = [
                    "setpriv",
                    "--reuid=65534",
                    "--regid=65534",
                    "--clear-groups",
                    *command,
                ]
            result = subprocess.run(
                command,
                env=dict(
                    os.environ,
                    PATH=str(root) + ":/usr/bin:/bin",
                    CALL_LOG=str(root / "calls"),
                ),
                stdin=subprocess.DEVNULL,
                timeout=10,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 7)
            self.assertIn("-Syu --skipreview", result.stdout)
            self.assertIn("system package update (exit 7)", result.stderr)
            self.assertFalse((root / "calls").exists())


class UpdateSetupFlowTests(unittest.TestCase):
    def run_script(
        self, decline_setup=False, fail_packages=False, nacre_installed=True
    ):
        temporary = tempfile.TemporaryDirectory(prefix="nacre-update-flow-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        root.chmod(0o777)
        tools = root / "bin"
        tools.mkdir()
        tools.chmod(0o777)
        log = root / "calls"
        counter = root / "package-queries"
        bodies = {
            "python3": 'case "$*" in *--needs-host) exit 1;; *--preflight|*--refresh) exit 0;; esac',
            "gum": (
                f'echo "gum:$*" >> "{log}"\n'
                f'if [[ "{int(decline_setup)}" == 1 && "$*" == *"Apply these setup changes?"* ]]; then exit 1; fi\n'
                "exit 0"
            ),
            "paru": (f'echo "paru:$*" >> "{log}"\nexit {7 if fail_packages else 0}'),
            "pacman": (
                f'echo "pacman:$*" >> "{log}"\n'
                'if [[ "$1" == -Qq ]]; then\n'
                '  if [[ "${2:-}" == nacre ]]; then\n'
                '    [[ "${NO_NACRE:-}" != 1 ]] || exit 1\n'
                "    echo nacre; exit 0\n"
                "  fi\n"
                '  [[ "${NO_NACRE:-}" == 1 ]] || printf "nacre\\n"\n'
                '  printf "niri\\norient\\n"; exit 0\n'
                "fi\n"
                f'count=$(cat "{counter}" 2>/dev/null || echo 0); count=$((count + 1)); echo "$count" > "{counter}"\n'
                'for package in "${@:2}"; do\n'
                '  if [[ "$package" == nacre && "${NO_NACRE:-}" == 1 ]]; then continue; fi\n'
                '  if [[ "$count" == 1 ]]; then printf "%s 1\\n" "$package"; else printf "%s 2\\n" "$package"; fi\n'
                "done"
            ),
            "nacre": (
                f'echo "nacre:$*" >> "{log}"\n'
                'if [[ "$*" == "setup --plan" ]]; then echo "plan shown"; else echo "setup applied"; fi'
            ),
            "flatpak": "exit 0",
        }
        for name, body in bodies.items():
            executable = tools / name
            executable.write_text("#!/usr/bin/env bash\n" + body + "\n")
            executable.chmod(0o755)
        script = root / "updates.sh"
        shutil.copy2(Path(__file__).parents[1] / "updates.sh", script)
        script.chmod(0o644)
        command = ["bash", str(script)]
        env = dict(
            os.environ,
            PATH=str(tools) + ":/usr/bin:/bin",
            HOME=str(root),
            CALL_LOG=str(log),
        )
        if not nacre_installed:
            env["NO_NACRE"] = "1"
        if os.geteuid() == 0 and shutil.which("setpriv"):
            command = [
                "setpriv",
                "--reuid=65534",
                "--regid=65534",
                "--clear-groups",
                *command,
            ]
        return subprocess.run(
            command,
            env=env,
            input="\n",
            capture_output=True,
            text=True,
            timeout=10,
        ), log

    def test_setup_plan_precedes_apply_and_compositor_change_prompts_relogin(self):
        result, log = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = log.read_text().splitlines()
        plan = calls.index("nacre:setup --plan")
        prompt = next(
            i for i, line in enumerate(calls) if "Apply these setup changes?" in line
        )
        apply = calls.index("nacre:setup")
        self.assertLess(plan, prompt)
        self.assertLess(prompt, apply)
        self.assertIn("plan shown", result.stdout)
        self.assertIn("setup applied", result.stdout)
        self.assertIn(
            "Changed packages:\nnacre 1 -> 2\nniri 1 -> 2\norient 1 -> 2",
            result.stdout,
        )
        self.assertIn(
            "Log out and back in to use the updated compositor.", result.stdout
        )

    def test_declined_setup_plan_does_not_apply(self):
        result, log = self.run_script(decline_setup=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = log.read_text().splitlines()
        self.assertIn("nacre:setup --plan", calls)
        self.assertNotIn("nacre:setup", calls)
        self.assertIn("left for later", result.stdout)

    def test_package_failure_skips_setup(self):
        result, log = self.run_script(fail_packages=True)
        self.assertEqual(result.returncode, 7)
        self.assertFalse(
            any(line.startswith("nacre:") for line in log.read_text().splitlines())
        )

    def test_unmanaged_nacre_skips_setup_review(self):
        result, log = self.run_script(nacre_installed=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = log.read_text().splitlines()
        self.assertFalse(any(line.startswith("nacre:setup") for line in calls))
        self.assertIn(
            "Nacre is not installed from its package; skipping the setup review.",
            result.stdout,
        )


class UpdateRuntimeTests(unittest.TestCase):
    def test_namespace_detection(self):
        for mapping, expected in (
            ("0 0 4294967295\n", False),
            ("1000 1000 1\n", True),
            ("0 1000 1\n", True),
        ):
            with patch.object(Path, "read_text", return_value=mapping):
                self.assertEqual(updates.needs_host_runtime(), expected)

    def test_native_package_commands_remain_direct(self):
        with patch.object(updates, "needs_host_runtime", return_value=False):
            self.assertEqual(
                updates.package_command(["checkupdates"]), ["checkupdates"]
            )

    def test_namespaced_check_preserves_output_and_failure(self):
        for code, output, error in (
            (0, "fish 4 -> 5\n", ""),
            (1, "", "Cannot fetch updates"),
        ):
            with (
                patch.object(updates, "needs_host_runtime", return_value=True),
                patch.object(
                    updates.shutil, "which", side_effect=lambda n: n == "checkupdates"
                ),
                patch.object(
                    updates.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], code, output, error),
                ) as run,
            ):
                if code:
                    with self.assertRaises(RuntimeError):
                        updates.preflight()
                else:
                    updates.preflight()
                self.assertEqual(
                    run.call_args.args[0],
                    [
                        "systemd-run",
                        "--user",
                        "--wait",
                        "--pipe",
                        "--collect",
                        "--quiet",
                        "--",
                        "checkupdates",
                    ],
                )

    def test_interactive_script_routes_before_confirmation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, body in {
                "python3": '[[ "$*" == *--needs-host ]]',
                "gum": 'echo "confirmation must happen in the host service"; exit 99',
                "systemd-run": 'printf "%s\\n" "$*"; exit 7',
            }.items():
                executable = root / name
                executable.write_text("#!/usr/bin/env bash\n" + body + "\n")
                executable.chmod(0o755)
            result = subprocess.run(
                ["bash", str(Path(__file__).parents[1] / "updates.sh")],
                env=dict(os.environ, PATH=str(root) + ":/usr/bin:/bin"),
                stdin=subprocess.DEVNULL,
                timeout=10,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 7)
            self.assertIn(
                "--user --wait --pipe --collect --quiet -- /usr/bin/bash", result.stdout
            )
            self.assertNotIn("confirmation must happen", result.stdout)
            self.assertIn("starting the host update service (exit 7)", result.stderr)


if __name__ == "__main__":
    unittest.main()
