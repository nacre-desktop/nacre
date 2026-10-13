# Community settings

These settings live on GitHub, outside the repository. This is the intended
experimental baseline and the maintainer's reapplication checklist. The direct
integration policy was applied and read back on 2026-10-13; recheck live settings
before treating this record as current. It remains in effect until changed by
the maintainer.
Repository workflows and templates are versioned separately.

## Main ruleset

In [Settings → Rules → Rulesets](https://github.com/nacre-desktop/nacre/settings/rules), create or edit the branch
ruleset **Main contributor checks**:

1. Set enforcement to **Active**, target `refs/heads/main`, and leave bypass actors
   empty. Do not restrict creation or deletion of ordinary task branches.
2. Enable **Restrict deletions** and **Block force pushes**.
3. Disable **Require a pull request before merging** and **Require status checks
   to pass** for this experimental development phase. Do not add a bypass actor
   or disable the whole ruleset; deletion and force-push protections stay active.
4. Maintainer agents validate changes locally, integrate with current `main`, and
   push directly without a PR or another permission prompt. Normal write access
   is still required; outside contributors continue using PRs.
5. Keep CI workflows enabled. Check published revisions and repair failures
   caused by the change. Current check names: `desktop`. CI is no longer a
   server-side gate before a direct push. Restore PR/status-check gates when the
   maintainer ends this experimental policy.

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

Expected: an active main ruleset with deletion/force-push rules and no PR or
required-status-check gate during experimental development,
`approval_policy: first_time_contributors`, private reporting enabled, Discussions
enabled, and all eight labels. Verify the three category names and formats in the
UI as well. If settings permissions are unavailable, include these exact manual
steps and the unapplied items in the policy change report before calling setup
complete.

GitHub documents the [rules API](https://docs.github.com/en/rest/repos/rules) and
[fork approval API](https://docs.github.com/en/rest/actions/permissions#set-fork-pr-contributor-approval-permissions-for-a-repository).
