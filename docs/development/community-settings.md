# Community settings

These settings live on GitHub, outside the repository. This is the intended
baseline and the maintainer's reapplication checklist. It was applied and read
back on 2026-10-12; recheck live settings before treating this record as current.
Repository workflows and templates are versioned separately.

## Main ruleset

In [Settings → Rules → Rulesets](https://github.com/nacre-desktop/nacre/settings/rules), create or edit the branch
ruleset **Main contributor checks**:

1. Set enforcement to **Active**, target `refs/heads/main`, and leave bypass actors
   empty. Do not restrict creation or deletion of ordinary task branches.
2. Enable **Restrict deletions** and **Block force pushes**.
3. Enable **Require a pull request before merging** and **Require conversation
   resolution before merging**. Required approvals: **0**. Dismiss stale reviews
   on new commits. CODEOWNERS requests maintainer review; mandatory code-owner
   approval and last-push approval are not enabled. This permits the sole
   maintainer to merge their own tested PRs while still requiring a PR and CI.
4. Enable **Require status checks to pass** and **Require branches to be up to
   date before merging**. Select the checks below, with GitHub Actions as the
   expected source (app ID `15368`). Do not require the manually dispatched
   `release` job. Keep required names synchronized with workflow job/matrix names.

- `desktop`

## Sensitive-path ownership

Nacre uses `@nacre-desktop/maintainers` in CODEOWNERS. The organization team is
visible (`closed`, not secret), contains the current maintainer, and has explicit
Write access to this repository so GitHub can request reviews. Keep membership
limited to trusted maintainers; do not add outside contributors automatically.
Using a team also keeps account names out of the public runtime-name policy.

If recreating it, open the organization's Teams page, create **Maintainers** as a
visible team, add the repository maintainer, and grant Write access only to Nacre.
Verify the CODEOWNERS errors endpoint on the proposed branch before merging:

```sh
gh api 'repos/nacre-desktop/nacre/codeowners/errors?ref=main'
gh api orgs/nacre-desktop/teams/maintainers/repos/nacre-desktop/nacre \
  -H 'Accept: application/vnd.github.v3.repository+json' \
  --jq '{full_name, permissions, role_name}'
```

## Fork workflows and private reporting

In [Settings → Actions → General](https://github.com/nacre-desktop/nacre/settings/actions), under **Fork pull
request workflows from outside collaborators**, choose **Require approval for
first-time contributors** (not just new GitHub accounts), then Save. Inspect
untrusted code before approving its workflow; execution approval is not merge
approval. Existing workflow token permissions are not broadened by this setting.

In [Settings → Advanced Security](https://github.com/nacre-desktop/nacre/settings/security_analysis), enable
**Private vulnerability reporting**. Confirm the repository's Security tab offers
[Report a vulnerability](https://github.com/nacre-desktop/nacre/security/advisories/new). If the UI wording moves,
use the private-reporting toggle on the repository's security settings page.

## Discussions and labels

In [Settings → General](https://github.com/nacre-desktop/nacre/settings), under **Features**, enable **Discussions**.
On [Discussions](https://github.com/nacre-desktop/nacre/discussions), open the category edit control and ensure:

| Category      | Format                | Use                                            |
| ------------- | --------------------- | ---------------------------------------------- |
| Q&A           | Question and answer   | Help, with answers that can be marked accepted |
| Ideas         | Open-ended discussion | Early proposals and tradeoffs                  |
| Show and tell | Open-ended discussion | Community projects and examples                |

Keep other existing categories unless there is a separate reason to change them.
Enabling Discussions normally creates these default categories; verify them rather
than creating duplicates. See GitHub's
[category guide](https://docs.github.com/en/discussions/managing-discussions-for-your-community/managing-categories-for-discussions).

Create missing labels in [Issues → Labels](https://github.com/nacre-desktop/nacre/labels): `good first issue`,
`help wanted`, `hardware`, `compositor:hyprland`, `compositor:niri`, `translation`,
`design`, and `performance`. These classify work; they do not imply compositor or
hardware support. Preserve other labels.

## Read back with gh

```sh
gh api repos/nacre-desktop/nacre/rulesets
gh api repos/nacre-desktop/nacre/rules/branches/main
gh api repos/nacre-desktop/nacre/actions/permissions/fork-pr-contributor-approval
gh api repos/nacre-desktop/nacre/private-vulnerability-reporting
gh repo view nacre-desktop/nacre --json hasDiscussionsEnabled
gh label list --repo nacre-desktop/nacre --limit 100
```

Expected: an active main ruleset, PR/status-check/deletion/force-push rules,
`approval_policy: first_time_contributors`, private reporting enabled, Discussions
enabled, and all eight labels. Verify the three category names and formats in the
UI as well. If settings permissions are unavailable, include these exact manual
steps and the unapplied items in the policy PR before calling setup complete.

GitHub documents the [rules API](https://docs.github.com/en/rest/repos/rules) and
[fork approval API](https://docs.github.com/en/rest/actions/permissions#set-fork-pr-contributor-approval-permissions-for-a-repository).
