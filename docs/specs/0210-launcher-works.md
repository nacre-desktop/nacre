# N0210 — Launcher works end to end

## Bugs and fixes

| Report | Cause | Fix / status |
| --- | --- | --- |
| `:` file hint remained on Home | Daemon looked up provider messages by term only; `:` and Home both have the empty term. | Attach cached feedback only for the active files/Brain scope. Empty files scope now lists existing search roots with the short query hint. |
| `>` caret below and right of the text | Reproduced in Bead `TextInput`, pinned at `ed03c8e`: with input bounds `(12,523,1068,49)`, the actual caret after `>` is `(243.8,563,1,20.53)`. It extended below the input and sat far past the glyph. | Bead B0233 fixes the caret. Nacre pins `ed5d1b8fc814b5f0a2dda23dac4dda3ff4c62849` and re-enables the headless caret test. |
| Empty `?`/`#`/`:` scopes were empty or showed a typing wall | Prefix providers only returned prompt-specific results; files rejected terms under two characters and AI returned no defaults. | `?` lists New chat, Resume chat, Browse chats, and an Ask scope row; `#` lists Brain actions plus recent notes; `:` lists existing folders plus a short hint. AI/Brain rows retain N0200 capability gates. |

## Runtime sweep

The existing handler audit fixture contains **103 catalogue actions** and **44 provider rows** (147 operation rows); its real `launcher_actions::plan` test checks each resolves to a supported route, command, daemon method, toggle, or form. This is handler-contract evidence, not the requested private-HOME daemon activation sweep.

| Changed entry | Cause and fix | Runtime result |
| --- | --- | --- |
| `?` → Ask | Empty AI scope had no defaults; retained the existing capability-gated scope handler and added the Ask row. | 7 prefix rows; activation outcomes included in totals below. |
| `#` → Brain | Empty Brain scope had no defaults; added actions and recent notes while retaining capability/source gates. | 3 prefix rows; activation outcomes included in totals below. |
| `:` → Files | Empty files scope had a typing wall; list existing roots and keep the short search hint. | 2 prefix rows; roots are openable. |
| Files feedback → Home | Cached provider feedback matched the empty term regardless of scope; only attach it in files/Brain scope. | `:` hint shown; no stale Home hint. |

The private-HOME runtime sweep queried 9 rail groups, 43 catalogue groups, 26 provider routes and all 9 prefix cases. It activated **496 times**: 482 unique menu rows through `menu.activate` and the real shell provider, plus 11 rail entries and 3 quick controls through real pointer events.

| Outcome (before Step C fixes) | Count |
| --- | ---: |
| Ran | 316 |
| Opened | 64 |
| Handled | 41 |
| Disabled by capability/state gate | 51 |
| Error | 22 |
| Informational | 2 |
| Nothing happened | 0 |

The 22 initial errors were 17 `Catalog changed; query again`, one `Invalid launcher action` (Rosé Pine bundle), one `lock was not confirmed` (private fake lock command), one `stay-awake.state is unavailable`, one `network.wifi is unavailable`, and one `Control backend is unavailable`. Step C fixes the first 18; the remaining hardware/service reports came from unavailable test backends, and the fake lock couldn't confirm a private session lock.

## Follow-up fixes and installed-program check

| Finding | Cause and fix / result |
| --- | --- |
| Repeated click after catalog update | Activation surfaced `stale_revision`, cleared rows, and returned an error toast. The shell now re-queries the same request and retries the same row id once on the same daemon boot; the two-activation regression verifies the refreshed revision. A changed query, pending navigation or daemon restart is not replayed. |
| Rosé Pine bundle rejected | Theme provider rows set `bundle: true` but omitted required `bundleScope`, and the planner rejected the `bundle:` prefix. Bundle rows now include `bundleScope: "all"`; the planner accepts scoped bundle ids and routes them to `software-library bundle-apply`. Provider and shell-plan tests cover the action. |
| `localsend` absent from installed PATH | Optional share dependency; launcher entry already uses the `share` workflow capability, which is computed from executable availability. It remains gated while absent. |
| `voxtype` absent from installed PATH | Optional dictation dependency; toggle/setup rows already use the `dictation` capability, computed from executable availability. They remain gated while absent. |
| Nacre executables | `nacre-helper` resolves to the installed `~/.local/share/nacre/shell/tools/nacre-helper`; `nacre-shell`, `nacre-terminal`, `nacre-ai`, `nacre-app`, and `xdg-open` resolve through the installed shell/AI private bins. No missing Nacre executable found. `n0210-test-app` was only the sweep fixture. |

The read-only program check used the installed systemd user-session `PATH`, plus `~/.local/share/nacre/shell/bin`, `~/.local/share/nacre/shell/tools`, `~/.local/share/nacre/palette-runtime/venv/bin`, and the installed AI conversation runtime bin; it ran `command -v` only and did not invoke or modify installed programs. The private sweep's missing fake NetworkManager/control services and fake lock confirmation were not product executable gaps.

## Checks

- `nice -n 10 cargo update -p bead --precise ed5d1b8fc814b5f0a2dda23dac4dda3ff4c62849` updated the Bead lock entries, including its newly split `bead-services` dependency.
- `nice -n 10 cargo fmt --check` passed.
- `nice -n 10 cargo test -p nacre-menu --locked -j2` passed.
- Python launch checks: `test_software_*.py` had two 5-second CLI timeouts under the combined suite; each failed test passed on an isolated rerun. A later combined run with temporary timeout changes passed 16 tests, but those out-of-scope test edits were reverted as directed. `test_theme_bundles.py` passed (2 tests).
- `RUST_MIN_STACK=16777216 timeout 900 nice -n 10 cargo test -p nacre-shell --bin nacre-shell --locked -j2 -- --test-threads=6` ran 881 tests: 875 passed, 2 failed outside N0210, 4 ignored. The failures were `app::tests::a_missing_app_is_started_with_its_page_socket_and_daemon` and `app::tests::widgets_app_receives_its_initial_search_argument`.
- `nice -n 10 cargo clippy -p nacre-menu -p nacre-shell --all-targets --locked -j2 -- -D warnings` passed.
- Step C: Bead pin advanced to `ed5d1b8fc814b5f0a2dda23dac4dda3ff4c62849`; the caret reproducer is enabled and passes. No goldens changed.
- Runtime sweep: `python /task/launcher-runtime-sweep.py` inside the private `dbus-run-session`, real daemon/shell, temporary HOME, fake command PATH and private `systemd-run` D-Bus manager completed; 496 activations recorded. Raw per-entry records and logs stayed in ignored `.lead/` / dev-1 task evidence.
- Installed-program check: read-only `command -v` over all observed launch commands and wrapper targets; only optional `localsend` and `voxtype` and the private sweep fixture were absent, with the two optional entries capability-gated.
- Pinned Luau analyzer 0.742: `python tools/check_product_shell.py --analyzer /tmp/luau-analyze --binary /target/debug/nacre-shell` passed with `LUAU_ANALYZE_OK 189 modules`; optional wallpaper smoke reported `SKIP missing sway`.
- No goldens changed.

With `RUST_MIN_STACK=16777216`, the final whole-shell run avoided the earlier rustc SIGSEGV. It passed 875 tests and failed two unrelated app tests: `a_missing_app_is_started_with_its_page_socket_and_daemon` and `widgets_app_receives_its_initial_search_argument`. The first also failed in its isolated rerun; the second was not rerun in isolation before the lead's one-run limit. The wallpaper-refresh and notice-service tests had earlier failures, then both passed their isolated reruns. All out-of-scope test files remain unchanged.

Step C product source delta: **+173 / −14 = net +159 lines**. `Cargo.toml`/`Cargo.lock` added 33 and removed 17 lines for the Bead pin. Tests and documentation are separate. No temporary duplicate code or assets added.
