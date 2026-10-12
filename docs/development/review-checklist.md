# Pull request review checklist

Apply this checklist to every Nacre PR, whether authored or reviewed by a human
or an AI. Read the entire diff, including generated assets and workflow changes.
Mark non-applicable checks with a reason in the review. An AI review assists the
maintainer; it does not supply permission or prove that a real-session test ran.

## Every PR

- [ ] The problem, resulting behavior and scope are clear. Inspect all changed
      files for broad unrelated edits, hidden behavior changes or unexplained
      generated output. Split unrelated changes before merging.
- [ ] The branch is current with `main` and required CI passes at the revision
      being merged. Read lint/format/test results and relevant logs, including
      skips; a green summary does not replace tests of the affected behavior.
- [ ] New code has meaningful tests; reproduction and regression evidence cover
      the changed path. Confirm claimed real-session/environment checks and their
      versions. Docs-only work checks links, forms and policy accuracy instead.
- [ ] Affected docs and public contracts are updated. UI changes include readable
      before/after screenshots using public demo data and respect translation
      conventions; flag unexplained hard-coded visible strings.
- [ ] The author can explain and has tested the work, including AI-assisted output.
      Separate measured results, reported observations and assumptions.
- [ ] Licenses and provenance permit contribution/redistribution; check third-party
      notices, image licenses and optional DCO trailers without requiring sign-off.
- [ ] No personal profiles, private settings, account state, memories, credentials
      or private images are committed or shown in logs/screenshots. Redact secrets
      and report suspected vulnerabilities through [SECURITY.md](../../SECURITY.md).

## Changes needing extra scrutiny

- [ ] Installer/configure/recovery changes are reviewed as a plan before apply:
      inspect destinations, symlinks, file permissions, idempotence, local drift,
      backups and rollback. Stage to an empty directory before a separate test
      account; never use the maintainer's daily profile as a test fixture.
- [ ] Sudo/pkexec or package-manager operations have explicit intent, safe argument
      handling and an appropriate authorization boundary. No protection or
      confirmation is bypassed to make installation/tests pass.
- [ ] AI permissions remain explicit opt-in at the appropriate profile boundary;
      check default profiles, launch flags and subprocess permissions. Do not
      silently enable full access or change personal settings.
- [ ] Brain auth, credential storage, HTTP binding and discovery retain their
      security boundaries; review unauthenticated routes, cross-origin requests,
      file exposure and secret-service integration.
- [ ] Orient pin, lock and adapter remain consistent when changing the engine;
      do not copy engine code into this repository.
- [ ] CI/workflow changes use minimum token permissions and trusted pinned actions.
      Review triggers, shell quoting, caches/artifacts and secret access. Never run
      untrusted fork code with write tokens/secrets via `pull_request_target` or an
      equivalent privileged path. Inspect code before approving a first-time
      contributor's workflow; that approval is separate from PR approval.
- [ ] New dependencies and downloads explain necessity, alternatives, source,
      version pin/integrity, license and execution/install hooks. Review lockfile
      and submodule changes; no unexplained network/sudo/dependency additions.
- [ ] New network calls explain destination, data sent, user consent, timeouts,
      retries and offline behavior. No hidden telemetry or transmission of private
      data; avoid secrets in URLs, process arguments or logs.
- [ ] Credential/auth changes use the existing OS secret store and permission
      boundaries; no plaintext credentials, copied authentication or weakened
      validation. Check exposure in errors, environment variables and subprocesses.
- [ ] Binaries/minified/encoded code have a stated source, license, build procedure
      and verification evidence. Inspect the source diff; reject unexplained opaque
      payloads rather than trusting generated output or an AI summary.

## Before merge

- [ ] Resolve outstanding review findings and confirm final CI on the current
      revision. Sensitive paths have maintainer scrutiny, including anything newly
      run with sudo; add explicit CODEOWNERS patterns when introducing such paths.
- [ ] Check deployment/recovery impact and rollback where applicable. Do not
      merge based on an unverified claim of live testing or bypass a failing check.
- [ ] Verify repository settings against the
      [community-settings guide](community-settings.md) when changing CI job names,
      workflows or community policy; required checks must match actual job names.
