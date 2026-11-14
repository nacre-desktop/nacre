from pathlib import Path
import subprocess
import unittest
import os
import shutil
import tempfile


class UpdateFailureTests(unittest.TestCase):
    def test_failure_trap_reports_step_exit_and_keeps_skipreview(self):
        import os
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root.chmod(0o777)
            for name, body in {
                "gum": "exit 0",
                "python3": '[[ "$*" == *--needs-host ]] && exit 1; [[ "$*" == *--preflight* ]] && exit 0; exec /usr/bin/python3 "$@"',
                "paru": 'printf "%s\\n" "$*"; exit 7',
                "nacre-helper": '[[ "$*" == "aur-review policy" ]] || exit 2; echo \'{"mode":"native"}\'',
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
                    NACRE_HELPER_BINARY=str(root / "nacre-helper"),
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
        self,
        fail_packages=False,
        nacre_installed=True,
        aur_mode='{"mode":"required"}',
        helper=None,
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
            "python3": 'case "$*" in *--needs-host) exit 1;; *--preflight*|*--refresh*) exit 0;; esac; exec /usr/bin/python3 "$@"',
            "gum": f'echo "gum:$*" >> "{log}"',
            "paru": (f'echo "paru:$*" >> "{log}"\nexit {7 if fail_packages else 0}'),
            "pacman": (
                f'echo "pacman:$*" >> "{log}"\n'
                'if [[ "$*" == "-Qq nacre" ]]; then\n'
                '  [[ "${NO_NACRE:-}" != 1 ]] || exit 1\n'
                "  echo nacre; exit 0\n"
                "fi\n"
                'if [[ "$*" == -Qq ]]; then\n'
                '  [[ "${NO_NACRE:-}" == 1 ]] || printf "nacre\\n"\n'
                '  printf "niri\\norient\\n"; exit 0\n'
                "fi\n"
                '[[ "$1" == -Q && $# -gt 1 ]] || { echo "error: unexpected call" >&2; exit 1; }\n'
                f'count=$(cat "{counter}" 2>/dev/null || echo 0); count=$((count + 1)); echo "$count" > "{counter}"\n'
                'for package in "${@:2}"; do\n'
                '  if [[ "$package" == nacre && "${NO_NACRE:-}" == 1 ]]; then continue; fi\n'
                '  if [[ "$count" == 1 ]]; then printf "%s 1\\n" "$package"; else printf "%s 2\\n" "$package"; fi\n'
                "done"
            ),
            "nacre": (
                f'echo "nacre:$*" >> "{log}"\n'
                '[[ "$*" == setup ]] || { echo "unexpected nacre call" >&2; exit 2; }\n'
                'echo "setup applied"'
            ),
            "flatpak": "exit 0",
            "nacre-helper": (
                '[[ "$*" == "aur-review policy" ]] || { echo "Unsupported helper" >&2; exit 2; }\n'
                f"[[ '{aur_mode}' == unreadable ]] && exit 1\n"
                f"printf '%s\\n' '{aur_mode}'"
            ),
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
            NACRE_HELPER_BINARY=str(helper or tools / "nacre-helper"),
            XDG_CONFIG_HOME=str(root / ".config"),
        )
        if helper:
            profile = root / ".config/nacre/profile.json"
            profile.parent.mkdir(parents=True)
            profile.write_text('{"version": 1, "id": "default", "aurReview": "native"}')
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

    def test_one_setup_run_reviews_and_applies_and_prompts_relogin(self):
        result, log = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = log.read_text().splitlines()
        # setup shows its plan and asks once; updates.sh adds no second question.
        self.assertEqual([c for c in calls if c.startswith("nacre:")], ["nacre:setup"])
        self.assertFalse(any("setup" in c for c in calls if c.startswith("gum:")))
        self.assertIn("setup applied", result.stdout)
        self.assertIn(
            "Changed packages:\nnacre 1 -> 2\nniri 1 -> 2\norient 1 -> 2",
            result.stdout,
        )
        self.assertIn(
            "Log out and back in to use the updated compositor.", result.stdout
        )

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

    def test_paru_follows_aur_review_policy_and_defaults_to_review(self):
        for policy, expected_args, expected_mode in (
            ('{"mode":"native"}', "paru:-Syu --skipreview", "native (skipreview)"),
            ('{"mode":"required"}', "paru:-Syu", "required"),
            ("not-json", "paru:-Syu", "required"),
            ("unreadable", "paru:-Syu", "required"),
        ):
            with self.subTest(policy=policy):
                result, log = self.run_script(aur_mode=policy)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(expected_args, log.read_text().splitlines())
                self.assertIn(f"AUR review mode: {expected_mode}.", result.stdout)

    @unittest.skipUnless(
        Path(os.environ.get("NACRE_HELPER_BINARY", "/nonexistent")).is_file(),
        "Candidate native helper required",
    )
    def test_real_helper_policy_keeps_skipreview_for_native_profiles(self):
        result, log = self.run_script(helper=os.environ["NACRE_HELPER_BINARY"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("paru:-Syu --skipreview", log.read_text().splitlines())
        self.assertIn("AUR review mode: native (skipreview).", result.stdout)


class UpdateRuntimeTests(unittest.TestCase):
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
