# N0204 — AI panel Tidy

Owner decision, 2026-11-12: “go with the tidy one; I like the part where it says
running or not; I like the new quick chat as well.” Implement option A from
`b48a2c2e0`, ported into the real view without merging the concept branch.

## What changed

The launcher puts the logo, smaller title, provider selector and pin on one row.
New chat and Quick chat sit together above a full-width search field with an
inset refresh icon. The newest unfiltered row offers Continue through the existing
resume-latest action; search results retain their exact-row open action.
Recent rows use gaps, subtle selection/hover fills and
status dots; Brain and Settings share the footer equally.

The conversation has a compact back/title/provider/status header and an overflow
menu for Workspace and New chat. Copy appears on message hover without reserving
empty bubble space. Send sits inside the composer; attachment, screenshot and
latest controls sit below it. Geometry uses theme tokens and derived values.
The existing actions, keyboard navigation and detached view remain supported.
No chat data/model changes or concept-option files are included.

The pinned transcript component always reserves an action strip. A product
adapter caches compact row heights and maps the visible window back to that
component, keeping its Markdown renderer and off-screen cell retirement. This
removes the Copy gap without a framework change or a second layout implementation.
There are no temporary replacement paths to retire.

## Before and after renders

Local review evidence lives in `.lead/renders/` (intentionally untracked):
`n0204-before-chat-{state}-{scale}.png` and
`n0204-after-chat-{state}-{scale}.png`. States: launcher, launcher-detached,
empty, conversation, code, approval, launcher-warning-tooltip; scales: 1 and 1.5.
The approved option A renders remain in the design unit's evidence directory.
Both implementer and coordinator inspected all 14 regenerated images: icons,
text, header spacing, compact bubbles and composer controls are intact at both
scales. The list's bottom edge retains its normal scroll-viewport crop.

| View (scale 1) | Before | After |
| --- | --- | --- |
| Launcher | [PNG](../../.lead/renders/n0204-before-chat-launcher-1.png) | [PNG](../../.lead/renders/n0204-after-chat-launcher-1.png) |
| Conversation | [PNG](../../.lead/renders/n0204-before-chat-conversation-1.png) | [PNG](../../.lead/renders/n0204-after-chat-conversation-1.png) |

## Checks

Focused checks passed in fresh dev-1 containers through the shared slot helper
(last run: 571 GB free; target inside the container; `nice -n 10`, Cargo `-j2`):

- `cargo test -p nacre-shell --bin nacre-shell --locked -j2 chat::tests:: -- --nocapture`: 23/23, including both-scale golden comparison, hover, Tab-to-Copy, menu and composer bounds.
- Same command with `reload::chat_runtime_tests::`: 8/8; the isolated cache case also passed.
- `cargo fmt --all --check` and `cargo clippy -p nacre-shell --all-targets --locked -j2 -- -D warnings`: passed.
- `nacre-shell --check shell/chat/main.luau`: `CHECK_OK`; `tools/check_luau.py --check-generated` with analyzer 0.742: `LUAU_ANALYZE_OK`, 189 modules.
- Ruff 0.16.10 `format --check` / `check`, and `python tools/compositor_boundary.py`: passed.

The fake-service refresh timing asserted 300 ms on a shared debug runner and
failed at 1,262 ms in comparison and 657 ms in isolation. Following N0173, the
refresh budget and 30 ms cached-frame budget require `NACRE_TEST_PERF=1`. Normal
tests still require completion within two seconds, cached content on the first
projection, and actions while refresh is pending. No product timeouts changed.
The final run measured 12 ms cached first-frame and 167 ms refresh; this unit
makes no controlled performance claim.

Product delta: +293 / −131 lines, net +162. The additional adapter keeps the
pinned framework's cached/windowed layout while removing reserved Copy space;
the replaced view code is deleted. Hosted CI on main remains the full gate.
Local commit only; no live desktop changes or publication.
