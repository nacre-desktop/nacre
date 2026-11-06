# N0144 — Brain Wiki tab and connections

Brain overhaul U6 (DESIGN §3, §7, §11). The owner asked for "one page that's
like a wiki, … a more readable summary place for the brain", with the regular
views staying the detailed layer, and for the Brain to be "better at
connecting different nodes". This unit adds a Wiki tab that the viewer opens
on, and derived connections (mentions, backlinks, related notes) shown in the
reader, on wiki pages and in the graph. Nothing is written into notes.

## Wiki tab

- **Landing view.** The viewer opens on Wiki (rail link, first tab, brand
  button). Overview, Connections and Notes stay one click away in the same
  tab bar; Graph stays in the toolbar. Rail world/area clicks keep the current
  mode (Wiki pages in Wiki, the detailed overview otherwise).
- **Home.** One card per registry world (note count, the world page's first
  paragraph, up to six area chips, `New:` for proposed areas), "Other pages"
  for unplaced wiki pages, and "Recently saved" notes. Cards render from the
  first-paint summary at once; `/api/wiki` adds page summaries.
- **Pages.** World → area → entity, with breadcrumbs. Each page shows the
  intro line, then **Current facts** first, then sub-pages (areas of a world,
  entity pages of an area, child entities of an entity), the remaining curated
  sections as Markdown (headings, lists, links, tables, code), a
  Status/Reviewed/Also-known-as line with "Open the full note", and a
  generated **Evidence** index: latest notes, digests, Linked from and Related,
  each row opening the detailed note reader. The page's own generated
  `brain:generated` block is not repeated; its content is computed live.
- **Placement.** Pages come from `wiki/` (no symlinks, hidden files or
  `Entity: alias`/`Redirect:` pages). World/area pages are the pages whose
  `Entity:` is world/area and whose name resolves through the registry
  (`brain_search::Categories`); entity pages are placed by `Worlds:`/`Areas:`,
  then `Parent:` (an area, world or another entity; children inherit the
  parent's placement). The migration's generated stubs (N0141) fit as is.
- **Fact lines.** `- X is Y. (confirmed 2026-11-05; [[devices/…]])` renders the
  text as inline Markdown, then a muted date (verb as tooltip) and the source
  link. `~~X~~ (… ) (superseded 2026-11-05 by [[…]])` renders struck through
  with "Superseded <date>" and the superseding link. `### ` lines group facts;
  other lines in the section render as Markdown ("Not yet curated …").
- **Staleness (render time).** DESIGN §7 classes: addresses/IPs/host state
  (IPv4/IPv6 literals, address, hostname, port, DNS, online/offline, uptime,
  mounted, reachable …) after 14 days; versions/revisions/deploy state
  (`rNNN`, `x.y.z`, `vN`, commit-like hex, version, revision, release,
  deployed, installed, pinned, firmware, kernel, build) after 30 days;
  everything else (decisions, preferences) only through supersession. A stale
  fact shows a "Stale" badge with the age and limit; cards count stale facts.
  Hook for N0145: `brain_wiki::staleness` returns
  `{class, days, limit, source: "render"}`; persisted flags use the same shape
  with `source: "index"` and replace the render-time value per fact.
- **Typography.** `.wiki-page` max width 75ch, 15 px / 1.7, intro 16 px muted;
  every colour is a theme token (light and dark verified).

## Connections (index time, derived only)

Built in `graph_at` with the existing background index, persisted in
`.brain-state/index/graph.json` (`GRAPH_SCHEMA` 2 → 3, so old caches rebuild)
and invalidated by the existing watcher (note revisions and the registry are
in the fingerprint). Module `brain_connections.rs`:

- **Mentions.** Targets: unmerged registry areas (names and aliases, including
  the aliases of areas merged into them) and wiki pages whose `Entity:` is not
  world, area or alias (name + `Aliases:`). One alternation, longest label
  first, word boundaries, labels of up to three characters case-sensitive
  (the registry's rule). Scanned text: title and body without the header and
  the generated evidence block. A page never mentions itself.
- **Links and backlinks.** The existing Obsidian-style resolver now lives in
  one `brain_web::Resolver` (+ `link_targets`) shared by the graph's source
  links and the wiki's link map; behaviour is unchanged.
- **Related notes.** Up to five per note, score ≥ 0.3:
  Σ IDF of shared mentions (capped at 2) + 0.25 for a shared area + 1.0 for a
  direct link + 0.15 per shared link target/backlink (≤ 0.6) + 1.2 × wording
  similarity; archived candidates × 0.6; ordinary notes do not list wiki
  pages (they appear as Mentions). Wording similarity is the new
  `brain_search::similar`: cosine over each note's 24 strongest title/body
  terms (TF-IDF from the existing search index, title doubled, terms in one
  note or > 10% of notes ignored). Each related row carries its reasons
  ("Shares Red Lantern · Same area: Alder Area 1 · Linked · Similar wording").
- **Graph.** `/api/graph` adds `mention` edges (each note's three rarest
  targets, skipping targets mentioned by > 20% of notes and areas the note
  already belongs to) and `related` edges (the top-five lists, deduplicated).
  They sit behind the existing kind filters (an edge shows when both ends
  do), only connect nodes already in a world/area scope, are dashed when
  related, and exert no layout force, so the structural layout and its fit
  are unchanged (the first attempt with force pulled wiki pages into one
  cluster and failed `browser-graph`'s fit check).

## Interfaces

- `GET /api/wiki` → worlds (registry order) with their page summary, areas
  (page summary, entity pages, `proposed`) and world-level pages; `other`;
  `pageCount`. Reads only `wiki/` and the registry.
- `GET /api/wiki/page?id=world:…|area:…|note:…` → title, kind, page, world,
  area, parent, meta (status, reviewed, source, aliases), intro, facts,
  factsNote, body, counts, subpages, links (curated link targets → note
  summaries), state, evidence (`null` while connections build; the viewer
  refreshes when they are ready).
- `GET /api/connections?id=note:…` → mentions, linksTo, linkedFrom, related
  (with reasons), or `{state:"building"}`. The reader's References / Linked
  from / Related / Mentions sections use it; `/api/brain` no longer carries
  the per-note connection record.
- Reader: "Open in Wiki" for wiki pages; mention chips open the entity or area
  wiki page. The obsolete graph-links "Referenced by" block and its message
  are removed (now "Linked from"). The reader's metadata fold also takes
  `Areas`, `Parent` and `Review` header lines (they were shown as body text).
- New messages: 33 `brain.ui.*` ids (en + translator context), generated into
  `brain/web/i18n-data.js`.

## Validation (dev-1, container `n0144`, 8 CPUs / 16 GiB, nice 10, cargo -j2)

- Unit: 3 connection tests (prose/header skipping; longest-first, bounded,
  case-sensitive short aliases, merged-area aliases, no self mention; related
  ranking, reasons, cap, backlinks, edge rules) and 3 wiki tests (fact
  provenance/supersession/stale classes/counts; section split without the
  generated block; world/area/entity placement, redirects hidden, sub-pages,
  parent entities, graph-backed link map, unknown ids).
- Browser (headless Chromium 1600×1000, synthetic vault, no owner data): new
  `brain/tests/browser-wiki.mjs` — registry with 7 worlds / 30 areas, 15
  entity pages (each with a fresh firmware fact, a 40-day-old address fact,
  a superseded fact and a preference; table and code in the body), 7 world and
  10 area pages, 2,000 linked notes, 6 digests. Wiki is the landing view
  (first paint 0.83 s, connections ready 2.76 s); world → area → entity
  navigation; facts render with provenance, one stale badge on the address
  fact (and on the 45-day-old area build fact), the superseded fact struck
  through with its link; the provenance link opens the reader, which shows
  Mentions, References, Linked from and Related with reasons; a mention chip
  returns to the wiki; Overview/Notes/Graph one click away; `/api/graph`
  carries mention and related edges (area scope: 879 related edges for 293
  nodes, ≤ 5 per node); 0 console errors. Existing `browser-hierarchy`
  (one added click on Overview, because Wiki now lands first), `browser-load`
  (unchanged; the wiki home shows recent notes at first paint),
  `browser-search` and `browser-graph` pass. Log:
  `.lead/evidence/n0144-browser.log`.
- Screenshots (`.lead/evidence/`, light and dark): `n0144-wiki-home-*`,
  `n0144-wiki-entity-*`, `n0144-wiki-evidence-*`,
  `n0144-reader-connections-*`.
- Full desktop CI: see the report (`.lead/report.md`).

## Size and cleanup

Product code +2,218 / −100 lines: `brain_wiki.rs` 699, `brain_connections.rs`
460, `brain_web.rs` +153/−74 (routes, shared resolver, edges), `brain_search.rs`
+72, viewer JS +588/−24, CSS +245. Tests: 210 unit lines, the 463-line browser
test. Removed: the reader's graph-links References/Referenced-by block and
`brain.ui.literal.referenced-by`; `source_links`' inline resolver (now the
shared `Resolver`). No new dependency, no temporary duplicate, no writes to
the vault.

Open: wiki prose links only explicit links (entity mentions in prose are not
auto-linked); persisted stale and contradiction flags arrive with N0145
through the `staleness` hook above.
