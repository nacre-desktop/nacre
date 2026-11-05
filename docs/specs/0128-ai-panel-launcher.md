# N0128 — Nacre AI launcher panel

Status: implemented on the owner-provided live-candidate base `59f6ce7f8`;
focused dev-1 validation passes. Topic branch only; no live installation.

The existing left AI route becomes a chat launcher. New chat offers a provider
and registered project; Resume latest and searchable recent chats open the normal
chat window, focusing an existing window where present. Brain, Settings and
Projects remain secondary actions. Quick chat expands the existing inline chat;
the launcher itself must not start a provider conversation.

The app and panel share the native AI dashboard's history discovery and launch
actions, including account and task-source handling. The daemon exposes bounded
`ai.launcher.*` calls; the panel keeps presentation and navigation in Luau using
Bead kit components and theme tokens. Detached panels use the same view/model.
No new chat engine, owner chat-data migration or installed configuration change.

Acceptance: fake-service model coverage for list/search/open/focus/new with
provider/project and Quick chat, explicit existing inline tests, launcher goldens
at scale 1 and 1.5, touched AI/shell tests and locked nacre-shell check. Full
desktop source CI runs if time permits. Validation is confined to one `n0128`
container on dev-1, with 8 CPUs/16 GiB and nice-10 commands; no laptop builds.

Design: retain the existing surface/primary/onSurface palette, IBM Plex Sans,
470-unit panel and kit pills/inputs/virtual list. Give recent chat titles the
largest content area; keep launch actions above search and utility actions below
the list. Selection and keyboard focus use the kit's existing palette roles.

Budget: bounded 30-chat model plus project registry, at most 1 MiB additional
always-open model memory; no performance claim without measurement. No framework
primitive is introduced. The inline transcript remains intentionally for Quick
chat, with no expiry because it is part of the approved feature.

The new daemon calls are `ai.launcher.list`, `.open`, `.new`, `.latest`, and
`.action`. Listing is single-flight, refreshed every 60 seconds or explicitly,
with at most 30 unique chats and 64 projects. Opaque stable keys resolve records
held in daemon memory. Discovery shares `ai_dashboard::collect_history` with the
app; window resumes share `resume_record`. `nacre-ai-chat serve` remains the
workflow MCP service. The old `ai.chat.saved` contract stays for existing inline
consumers. Quick chat alone renews the inline worker heartbeat.

Successful external-window actions dismiss an unpinned attached launcher to
release its exclusive keyboard input. Pinned and detached launchers remain open.
Existing tmux sessions attach with their original provider/login, including
session-only records whose provider thread ID is not known yet. Owned terminal
process metadata identifies existing local tmux clients. Records without tmux
sessions use the app's normal resume path (direct CLI for local Codex).

Validation in the task container: locked shell/helper checks; 19 panel/model
tests; 8 runtime and 2 pearl tests; 2 launcher protocol tests; 21 app model tests;
15 daemon chat and 4 terminal metadata tests; 14 AI command and 15 sidebar
integration tests pass. Helper/protocol Clippy passes. Main and detached
launcher goldens at scales 1 and 1.5 were reviewed, along with the deliberately
updated Quick chat goldens. Translation coverage passes. No runtime memory or
live desktop claim is made.

The broader desktop replica is recorded in `.lead/`. It finds three Clippy
argument-count warnings in unchanged placement/reveal helpers and six failures
in untouched Niri capability/frozen preparation tests. The full shell run also
finds untouched overview hover-title and dashboard CPU preparation failures;
the two task-related pearl golden differences were corrected and rechecked.
Source/UI checks stop on seven untouched Python formatting differences. The
additional complete catalog mirror check retains 151 English / 150 context
differences (152 / 151 on the starting tree); none were introduced here, and
all 19 new launcher labels/contexts match the canonical catalog.

The inherited missing
Niri test-fixture field and stale framework validation pin were repaired in
separate commits to make validation executable; frozen expectations were kept.
