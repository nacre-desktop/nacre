## Problem and change

Describe the concrete problem, resulting behavior, and scope. Link the issue or
discussion if there is one. Keep unrelated changes in separate PRs.

## Validation

List exact commands and results, relevant versions, and real-session checks.
For docs/policy-only changes, explain why runtime checks are not applicable.
Include screenshots for UI changes using public demo data.

## Risk and authorship

Explain new network calls, sudo/privileged operations, dependencies or downloads,
and any change to AI permissions, credentials or data handling. State none when
there are none. For AI-assisted work, explain what you verified yourself and why
the change is correct; the author owns the result.

## Checklist

Mark applicable items complete; explain any N/A in Validation or Risk above.

- [ ] The PR is small and focused; behavior changes are described.
- [ ] Lint, formatting and tests pass; new code has relevant tests.
- [ ] Tested on a real session or environment appropriate to the change.
- [ ] UI changes include before/after screenshots with private data removed.
- [ ] Affected documentation is updated.
- [ ] New or changed visible strings follow the translation conventions (or the
      current translation limitation is explained); no new hard-coded UI strings.
- [ ] New network access, sudo use, dependencies and downloads are explained.
- [ ] AI access remains explicit opt-in; no personal profiles, private settings,
      credentials or account data are included.
- [ ] I reviewed and can explain all submitted changes, including AI-assisted work.
- [ ] I read [CONTRIBUTING.md](https://github.com/nacre-desktop/nacre/blob/main/CONTRIBUTING.md) and the
      [review checklist](https://github.com/nacre-desktop/nacre/blob/main/docs/development/review-checklist.md).
