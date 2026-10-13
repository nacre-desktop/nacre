# Contributing to Nacre

General desktop work belongs here; color analysis and palette generation belong
in [Orient](https://github.com/nacre-desktop/orient). Keep personal profiles,
accounts, memories and private settings out of contributions.

## Start small

Use [Q&A](https://github.com/nacre-desktop/nacre/discussions/categories/q-a) for questions,
[Ideas](https://github.com/nacre-desktop/nacre/discussions/categories/ideas) for early proposals, and
[Show and tell](https://github.com/nacre-desktop/nacre/discussions/categories/show-and-tell) for community work.
Search existing issues before using the bug, feature or hardware report forms.
Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

Look for `good first issue` and `help wanted`. Keep PRs small and focused on one
problem; discuss broad changes first. A label describes a topic, not a promise
of platform support. Include reproduction steps and the user-visible result.

## License and optional sign-off

By submitting a contribution, you agree that it is provided under the repository's
GNU GPL version 3 only (GPL-3.0-only) [license](LICENSE). Only submit work you have the right to
contribute. Preserve copyright notices and document the origin and license of
third-party code, images and other assets; dependency licenses remain separate.

A `Signed-off-by` trailer following the
[Developer Certificate of Origin (DCO)](https://developercertificate.org/) is
welcome but optional. Use `git commit --signoff` if you can make that certification.
This sign-off is an authorship/rights statement, separate from cryptographic commit
signing; neither a mandatory DCO check nor a CLA is introduced here.

## AI-assisted contributions

AI-assisted PRs are welcome when the author has reviewed, tested and can explain
all changes. Say how AI was used, what you checked yourself, the test results and
any limitations. Generated explanations or passing CI alone are insufficient
for changes that need real-session validation. Never submit secrets, account
state or private source material to the repository or an AI service.

## Develop and validate

Fork the repository, create a task branch (and an isolated worktree for concurrent
work), and rebase on current `main` regularly. Add relevant regression tests for
new code, keep lint/format/tests passing, and update affected documentation.
Use public demo data for screenshots; remove private metadata and notifications.
Describe new dependencies/downloads, network calls and privileged operations.

See [the development guide](docs/contributing.md) for build dependencies,
installation staging, packaging and releases. Initialize submodules and use the
project's pinned tools before running:

```sh
PATH="$PWD/.venv/bin:$PATH" python tools/check.py
./install.sh --profile default --prepare /tmp/nacre-test-install
```

Use an empty staging directory and a separate test account for installation
changes. Desktop, hardware, lock/resume and performance changes need real-session
checks as well as unit tests. Preserve accounts, private settings, notes and busy
assistant sessions. AI full-access must remain an explicit profile opt-in.

Documentation-only changes should check links, templates and claims; explain
runtime checks that are not applicable instead of claiming they were performed.
During experimental development (effective 2026-10-13), maintainer agents should
complete requested work in isolated task worktrees, validate it, integrate it
with the latest `main`, and push directly without creating a PR or asking again
for commit/push permission. If another task advances `main`, fetch and integrate
again before a normal fast-forward push. Never force-push or overwrite another
task's work. Check CI after publishing and fix failures caused by the change.
This policy remains in effect until the maintainer changes it.

Outside contributors still submit PRs. When a PR is explicitly requested,
complete the [PR template](.github/PULL_REQUEST_TEMPLATE.md). All changes should
use the [review checklist](docs/development/review-checklist.md). Sensitive paths
retain [CODEOWNERS](.github/CODEOWNERS). Approving a fork workflow permits
execution, not merging.
