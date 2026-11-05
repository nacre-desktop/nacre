# N0128 — Nacre AI launcher panel

Status: implementing on the owner-provided live-candidate base `59f6ce7f8`.

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

Evidence and remaining limitations will be recorded after integrated validation.
