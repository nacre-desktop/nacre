# N0035 — UI language and message catalog for shells

Status: **Implemented; verification below; unpublished.** Consumer: Bead B0026
(`docs/specs/0026-keybind-routes-settings-integration.md` in Bead, "i18n" part).
Protocol types are MIT OR Apache-2.0 (`nacre-protocol::i18n`); JSON Schema, Luau
types and the golden transcript are generated under `protocol/`.

## Problem

Nothing published the UI language to Bead: `en.json` lived only in the Nacre
install, there was no topic, and Bead strings had no message IDs. QML and
`nacre-i18n` keep reading the install files; this unit adds the daemon-side
publication so any shell can translate without reading Nacre's install.

## Published interface (version 1, outside daemon safe mode)

Topic `i18n`: `{language, fallback, revision}`.

- `language`: the language the catalog serves: `en`, `pseudo` (developer mode only,
  as in Settings) or a lowercase tag such as `nb` / `pt_BR`.
- `fallback`: always `en`; ids a translation lacks are served from English.
- `revision`: starts at 1 per daemon run and increases by one each time `language`
  changes. It is the cache key for shells.

Method `i18n.catalog {namespace, revision?, after?}` (class `desktop`):

- `namespace`: the first dotted segment of an ID (`^[a-z][a-z0-9]{0,31}$`).
- `revision`: if present and different from the topic, the call fails with
  `stale_revision` (retry): refetch the topic, then the catalog.
- Result `{revision, entries, next?}`. `entries` maps id to a text or to
  `{one, other}` plural forms, sorted by id, at most **200 entries and 32 KiB** per
  page (ids + serialised texts; a single entry is at most 1024 bytes). When more
  remain, `next` is the last id returned: pass it back as `after`. An unknown or
  empty namespace returns `{entries:{}}`.
- Texts contain `{name}` placeholders that the shell substitutes. For `pseudo` the
  daemon returns already transformed text (placeholders and tags preserved), so
  shells do not implement pseudo.
- Errors: `validation` (bad params, unknown fields), `stale_revision`,
  `unavailable` (no settings/catalog service), `io`.

No catalog snapshot is on the bus. A shell subscribes to `i18n`, fetches the
namespaces it uses once per `revision`, and keeps them in memory. On a revision
change it drops the cache and refetches (live refresh); an English `tr(id, fallback)`
keeps working while a fetch is pending or fails.

## Source of truth

The language is the desktop settings store's `language` (`NACRE_LANGUAGE` overrides
it, as in QML and the helpers; `system` follows `LC_MESSAGES`/`LANG`, base language
only). The catalog is the existing install files: `en.json` overlaid by
`<language>.json` when present (the same loader the daemon launcher already used).
The daemon recomputes the language whenever the desktop store is published, which
covers `settings.set`, `nacre desktop-settings` and hand edits to `desktop.json`, so
there is no extra watcher or timer. Catalog files are read per request and not
cached: nothing is resident. Only English and (developer) pseudo are offered by
Settings today; a `<tag>.json` file is served when `NACRE_LANGUAGE=<tag>` selects it.

## Launcher rows

The daemon already localized catalog rows (`labelId`/`descriptionId`/`confirmId`
companions resolved by `nacre_i18n::localize` into the row text) and rebuilds the
launcher when `desktop.json` or a catalog changes. Rows are therefore delivered as
localized text, not IDs, to both shells; this unit only moved the language
resolution into one shared helper (`menu::message_language`) used by the launcher
and the new publisher, so both always agree. No row field was added.

## Bead string IDs

Bead-only strings live in the same `nacre/shell/i18n/en.json` and `context.json`
under the `bead.` namespace, so translators work in one place. ID format
`bead.<surface>.<name>[.<qualifier>]`, lowercase, digits and hyphens per segment:

- `<surface>`: the Bead UI area (`bar`, `panel`, `dashboard`, `launcher`, `osd`,
  `notifications`, `settings`, `common`).
- `<name>`: the stable meaning, not the English wording (`bead.bar.clock-tooltip`).
  Keep the ID when wording changes; do not reuse one for a different meaning.
- Plural messages use `{one, other}`; dynamic values are named placeholders.
- Every entry needs a translator note in `context.json`.

`tools/i18n/check.py` exempts `bead.` IDs from the "unused English ID" rule (their
references are in Bead's code) but still requires valid IDs and context. Bead's
`tr(id, fallback)` carries the English fallback, so an ID missing from the catalog
degrades to English. Seeding the actual `bead.*` entries is part of the Bead unit
that adds `tr()` (it owns the string inventory); no placeholder entries are added here.
Bead B0032 seeded 283 `bead.*` ids (the whole `bead` namespace is two catalog pages).

## Verification

Schema/golden export regenerated with the repo generator; `nacre-protocol` unit and
golden tests; daemon unit tests (paging, byte bound, pseudo, namespaces) and a
private-daemon test with a temporary HOME that changes `desktop.json` from English to
pseudo and observes the revision bump and `stale_revision`; translation-guard test
for the `bead.` exemption. Full desktop CI result is recorded in the unit report.

## Temporary duplicates

None. QML and `nacre-i18n` keep their own English readers (unchanged); no
second language resolver remains (the launcher uses the shared helper).
