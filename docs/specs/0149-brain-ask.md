# N0149 — Ask the Brain (U10)

Status: implemented. Final command results and evidence are recorded in
`.lead/report.md` and `.lead/n0149-full-ci.log`.

The viewer and `nacre-brain ask "question" [--json]` share a bounded,
read-only question backend. Ask is disabled by default. Before enabling it,
the viewer states: “Ask the Brain sends excerpts of matching notes to
Anthropic (Claude) using your subscription.” `--yes-once` permits one CLI
question without changing the saved setting. No timer or background model
work is added, and no live installation or private vault is changed.

## Contract

`GET /api/ask` returns `{enabled}`. `POST /api/ask/settings {enabled}`
persists the opt-in as `brain.ask.enabled` in the private, atomically written
`$XDG_CONFIG_HOME/nacre/brain.json` (default `~/.config/nacre/brain.json`).
This standalone viewer has no daemon settings record to extend naturally. `POST /api/ask {question}` returns `answer` (at most
1,200 characters), `citations` (wiki links), `confidence`, and `followups`.
The viewer renders model text as text, with citation buttons opening its
existing note reader. Follow-up buttons submit a new question.

All routes retain the existing loopback host, browser cookie, bearer and
same-origin checks. Disabled requests fail with a clear 403 error; busy
requests fail immediately, and model authentication/usage failures are
reported without exposing raw model output. One model request is allowed
per settings profile at a time, including across CLI and viewer processes.

## Evidence and model boundary

Retrieval uses the existing ranked search without changing its ranking algorithm,
then relevant world/area packs and wiki Current facts. Evidence is
deterministic: 12 ranked notes (headers and search snippets), relevant wiki
sections in path order (matched entities as well as scope pages), and exact
world/area packs in scope order. Only whole
entries fitting the 56,000-byte evidence cap become allowed citations
(roughly 16k tokens including request overhead). The search walker prunes
`credentials/` before opening files, including during index refresh. Credential paths and
secret-looking excerpts are excluded before the call. The fixed prompt
is stored under `ai/skills/nacre-brain-ask/`; evidence is untrusted data,
never instructions. The CLI runs Sonnet with no tools in an empty private
directory, without an API key, using the owner's subscription, with a
55-second timeout (the HTTP connection permits 75 seconds). It cannot read additional vault files through tools.

Rust validates output shape, length, citation membership and secrets.
Unknown citations reject the complete answer. Unsupported answers must
abstain instead of filling gaps from model knowledge. Citation membership
is a mechanical guarantee; it does not prove that a cited excerpt entails
every sentence. The fixed grounding prompt and synthetic negative tests
cover this remaining model behavior boundary.

## Verification and evidence

Synthetic fixtures and a stub `claude` only; never a real model in tests.
Required cases: valid answer, unknown citation, missing fact, injection
echo, auth failure, disabled refusal, opt-in and one-shot behavior.
Browser acceptance covers disclosure, enable, answer, reader citations,
follow-ups and zero console errors, alongside load/graph/hierarchy/search/wiki.
Light/dark 1600×1000 screenshots are recorded under `.lead/evidence/`.
Full desktop CI is replayed in dev-1's sole task container `n0149`
(Arch latest, 8 CPUs, 16 GiB, nice 10, Cargo -j2). Results are filled in
after execution in the report, not inferred from code inspection.
Screenshots: `.lead/evidence/n0149-ask-{light,dark}-{disabled,answer}.png`.
The browser harness runs under the desktop test host capture lock, with
private Node/Chromium binaries and profiles; Chromium is absent from CI.

## Size and cleanup

No always-on service is added; model scratch directories and processes end
after each call. Always-on memory budget: 0 bytes outside the existing
on-demand viewer/helper. The curator source is untouched while N0148b is
in flight. Any invocation logic temporarily mirrored in the new shared
module must be deduplicated by the lead: delete the old curator copy 7 days
after the shared replacement is trusted. Line counts and final verification
are recorded in the final report.
