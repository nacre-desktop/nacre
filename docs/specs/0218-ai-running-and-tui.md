# N0218 — AI activity and terminal dashboard

“Running” means the provider is working, rather than merely having an open
terminal session. Session attachment remains independent, so idle chats still
reopen their existing session. History listing and panel geometry are unchanged.

Claude activity comes from the existing bounded transcript reader: a pending
prompt, outstanding tool call, or progress after a completed turn indicates
work only while the session is alive. Completed assistant turns are idle.
Prompt/progress activity expires after five minutes without writes when no tool
is outstanding; unresolved tools remain active while their session is alive.
Partial assistant records do not end a turn. Codex reads at most the last 1 MiB
of the rollout named by `Thread.path`, tracking prompts, calls/results and
final/complete/aborted turns. Missing or unsafe rollout paths show Idle.
The panel refreshes history every ten seconds while open (plus request time).
Its fixed-size status dot fades on a 1.6-second cycle with 200 ms out-cubic
smoothing, only for Running rows. The existing heartbeat ticks at 50 ms while
the launcher shows work, ten seconds when idle, and stops when hidden.
No additional background service is introduced.
Cached rows start Idle until fresh provider activity is available.
Native Claude record structure was checked without retaining message content:
completed turns use `end_turn`; tool turns use `tool_use`.

The native dashboard and Load chat picker share centered geometry, adjacent
logo/title placement, full-width selection, terminal key decoding and SGR mouse
navigation. Input reads bypass stdin buffering so descriptor polling cannot miss
prefetched escape bytes. Terminal cleanup restores mouse reporting and modes on
ordinary exit, panic and handled termination signals. There are no replacement
paths or temporary duplicates to retire.

Validation passed in fresh dev-1 containers through the shared slot helper:
1,021 helper library tests (3 ignored), 8 PTY tests, and all 876 shell binary
tests (5 ignored), including status/cache, runtime routes and existing goldens.
Fmt, clippy on both touched crates, the native view check, Luau 0.742 analysis
(189 modules), Ruff 0.16.10 (586 files) and the compositor boundary check passed.
App-launch tests use the candidate helper with a private user-manager fixture.
An earlier unrelated wallpaper hydration flake passed alone and in the final
whole-shell run; no goldens changed. Hosted CI on main remains the full gate,
and the lead performs the owner's live audit. No live files changed.

Product delta: +608 / −94 lines, net +514. The additions implement the two
provider state machines, terminal protocol/signal handling and the pulse;
the old input decoder and row drawing are replaced rather than retained.
