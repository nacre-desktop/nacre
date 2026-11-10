# N0160 desktop checks

All check commands from `.github/workflows/check.yml`'s `desktop` job passed on
dev-1 in the task-private Arch Linux container (8 CPUs, 16 GiB, no swap).
The tracked checkout matched all 2,705 canonical entries, including the pinned
Orient gitlink; see [preflight](ci-preflight.txt). Production source was based on
main `5ae5309d1` plus N0160; later edits only record these results.

Changed production source SHA-256 (`crates/nacre-helper/src/lock_dashboard.rs`):
`dc3e5b5a487de96d6e69c9f595a8f39098da4edd9f178ea4b66d8dabe0a3fa76`.
Clean CI release-helper SHA-256:
`441436c274d7e7bff311c0d38fdef7b1afaae297688c30dced2688aa978e5b75`.
This is a separate, uninstrumented build from the matched benchmark binaries.

## Commands and outcomes

The complete commands are archived in [the desktop recipe](replay/full-desktop-ci.sh.txt).
Its SHA-256 is `e4c064e9ba44becd942cd4c3cbcd37c3015b7a56f5863042da09d9101b0c0dcc`.
Launch was `nice -n 10 docker exec n0160-lab bash -x /root/full-desktop-ci.sh`;
the script applies nice 10 once to each build/test command (verified in processes).

- `cargo fmt --check`, workspace/all-target Clippy with `-D warnings`, examples:
  passed.
- `cargo test --workspace --locked -j2`: passed, including the helper library's
  1,000 passing tests (three intentionally ignored) and all four shell tests
  that failed in the earlier nice-19 run.
- Minimal protocol consumer, release helper, trace memory-policy tests,
  no-default/all-features checks and all three shell/wallpaper builds: passed.
- Pinned Luau analyzer/product checks: `LUAU_ANALYZE_OK 189 modules`; private
  compositor/wallpaper smoke passed.
- Pinned cargo-deny: advisories, bans, licenses and sources all passed.
- `npm ci`, translation coverage (1,787 messages), generated assets: passed.

The main trace then stopped at the source guard because it also scanned
untracked benchmark scripts in `.lead`, absent from the clean workflow checkout.
Those files were preserved outside the checkout. No tracked source changed.
The [completion recipe](replay/ci-completion.sh.txt), SHA-256
`90a7276bc91f16196bf56e7a9e2c6f8e6f382bfce890b5436fe784913db065e9`,
reran the failed step and every remaining workflow step in order:

- `dbus-run-session -- xvfb-run -a /tmp/check-env/bin/python tools/check.py`:
  passed, including Python/JSON/shell/Lua/QML, compositor boundary, palette CLI,
  JavaScript/Prettier and four Python suites (404/one skipped, 138, 54, 376).
- `NACRE_HELPER_BINARY=... python tools/capture-aur-review.py --output ...`:
  passed without builds, installs or live-desktop access.
- `NACRE_MEMORY_BINARY=... cargo test -j2 --locked -p nacre-helper --test daemon
  memory:: -- --ignored --nocapture`: one passed, zero failed, 25.56 seconds.
  CPU renderer: peak anon 261,532 KiB; settled anon 20,476 KiB, RSS 54,364 KiB,
  clean file PSS 19,723 KiB.
- `git diff --exit-code` against the verified canonical index: passed.
- Completion recipe: `EXIT_CODE=0`.

The initial main-script nonzero exit is not claimed as a clean pass. Together,
the main and completion traces cover every desktop-job check successfully.
Container-only portal/FUSE warnings did not fail the source/UI suites.
Earlier shell-test failures and isolated reruns are described in [the method](README.md).

## Retained trace identities

Full traces are retained in the task worktree's ignored `.lead/` directory:

| File | SHA-256 |
| --- | --- |
| `n0160-full-desktop-ci.log` | `e88b933a5195d3862148f92ab94a2210199cded7c4cc94c294866e5e93a3ef10` |
| `n0160-ci-completion.log` | `ab2b73ded9b27c519e0a97bde93596e664a329e6c3ec32d587af593891d5ab1a` |

No test or build ran on the laptop or occupied desktop TEST host. Owner/lead
live audit remains outside this task's private-container evidence.
