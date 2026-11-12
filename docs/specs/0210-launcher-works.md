# N0210 — Launcher works end to end

## Bugs and fixes

| Report | Cause | Fix / status |
| --- | --- | --- |
| `:` file hint remained on Home | Daemon looked up provider messages by term only; `:` and Home both have the empty term. | Attach cached feedback only for the active files/Brain scope. Empty files scope now lists existing search roots with the short query hint. |
| `>` caret below and right of the text | Reproduced in Bead `TextInput`, pinned at `ed03c8e`: with input bounds `(12,523,1068,49)`, the actual caret after `>` is `(243.8,563,1,20.53)`. It extends below the input and sits far past the glyph. The Nacre input geometry is correct. | **Blocked on Bead / Platform ownership.** Kept an ignored headless reproducer in `launcher_pixels_tests.rs`; no Nacre geometry workaround. |
| Empty `?`/`#`/`:` scopes were empty or showed a typing wall | Prefix providers only returned prompt-specific results; files rejected terms under two characters and AI returned no defaults. | `?` lists New chat, Resume chat, Browse chats, and an Ask scope row; `#` lists Brain actions plus recent notes; `:` lists existing folders plus a short hint. AI/Brain rows retain N0200 capability gates. |

## Runtime sweep

The existing handler audit fixture contains **103 catalogue actions** and **44 provider rows** (147 operation rows); its real `launcher_actions::plan` test checks each resolves to a supported route, command, daemon method, toggle, or form. This is handler-contract evidence, not the requested private-HOME daemon activation sweep.

| Changed entry | Result / implementation |
| --- | --- |
| `?` → Ask | Existing `scope` handler sets `? ` in the query so typing continues; capability-gated with AI. |
| `#` → Brain | Existing Brain actions and recent-note provider rows; capability/source gates retained. |
| `:` → Files | Existing root folders activate as `open-file`; typed search still uses `fd` at two or more characters. |

**Phase 2 remains incomplete:** no real-daemon/fake-binary activation results are claimed. No additional entries were hidden or removed.

## Checks

- `cargo fmt --check` passed on dev-1.
- `cargo test -p nacre-menu --locked -j2` passed, including the empty-prefix visibility test and frozen provider tests.
- `cargo test -p nacre-helper --locked -j2 daemon::menu::tests::` and `cargo test -p nacre-helper --locked -j2 menu::cache_tests::empty_file_search_offers_existing_roots_as_openable_rows` passed.
- `cargo test -p nacre-shell --bin nacre-shell --locked -j2 launcher_` passed (73 passed, caret reproducer ignored); existing launcher route audit covered 103 catalogue actions and 44 provider rows.
- `cargo clippy -p nacre-menu -p nacre-helper -p nacre-shell --all-targets --locked -j2 -- -D warnings` passed.
- Pinned Luau analyzer 0.742: `python tools/check_product_shell.py --analyzer /tmp/luau-analyze --binary /target/debug/nacre-shell` passed with `LUAU_ANALYZE_OK 189 modules`; optional wallpaper smoke reported `SKIP missing sway`.
- Caret reproducer was run explicitly and failed with the coordinates above, confirming the Bead-owned blocker. No goldens changed.

Product delta: +66 / −17 = **net +49 lines**; test and documentation lines excluded. No temporary duplicate code or assets added.
