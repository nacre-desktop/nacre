# N0217 Brain as a standalone Bead app

Status: design (two directions rendered, nothing integrated). Owner 2026-11-12: the Brain should be
"an actual standalone app instead of a Brave thing, maybe built with Bead", and Connections should
become "a gigantic internet of nodes": an Obsidian-style force-directed graph of all ~2,000 notes
and their ~10,000 links. The graph is designed against Bead's planned `kit.GraphCanvas` (B0232,
`docs/specs/0232-graph-canvas.md` on Bead main 84513eec, not built yet); everything else uses
today's kit.

## 1. Inventory of the web Brain

Today: `brain/web/*` (app.js 2,614 lines, graph.js 735, style.css 1,664, i18n.js +
translation-engine.js, vendored d3-force) served by the on-demand, idle-exiting (300 s) worker
`nacre-helper brain-control serve` (N0024: `brain_web.rs` routes, `brain_http.rs` loopback HTTP,
`brain_auth.rs` cookie + bearer + bootstrap), opened in a Chromium-family app window by
`brain_launch.rs` (daemon `brain.open`, `nacre-brain open`, Super+B, `nacre-brain.desktop`).
The web app polls `/api/navigation` every 1 s and `/api/brain` every 30 s while visible.

| Screen / feature | Web code (app.js) | Helper API today | In the Bead app |
|---|---|---|---|
| Wiki home: worlds, areas, counts, "new" badges, rail | `renderWikiHome`, `renderRail` | `GET /api/brain` (graph + hierarchy + review count), `/api/wiki` (`brain_wiki`) | Overview + rail, phase 1 |
| Overview: subjects, recent knowledge, across subjects | `renderMain` | `/api/brain`, `/api/summary` (`retry=1` reschedules the graph build) | Overview, phase 1 |
| Wiki page / note reader: facts, evidence, superseded, loops, inspector | `renderWiki`, `renderFacts`, `renderEvidence`, `renderLoopsPanel`, `renderInspector` | `/api/wiki/page?id`, `/api/note?path` (raw text, `NOTE_LIMIT`) | Note reader, phase 1 |
| Notes list + filters (world, kind, since, archive, review focus) | `renderNotes`, `renderNotesFilters` | `/api/brain` projections, `/api/search` (`limit/since/archive`) | Search/Notes, phase 1 (`VirtualGrid`) |
| Search (Ctrl K) + results | `renderResults` | `/api/search?q` (`brain_search`, optional semantic worker) | Search, phase 1 |
| Ask the Brain: opt-in, question, cited answer | `submitAsk`, `openAskCitation`, `refreshAskSetting` | `GET/POST /api/ask`, `POST /api/ask/settings` (`brain_ask`, `brain_claude`) | Search > Ask panel, phase 1 |
| Connections: zoomed-in world cards, shared subjects, confidence | `renderMap`, `sharedSubjects`, `confidenceLabel` | `/api/connections?id` (`brain_connections`) | Replaced by Graph (full + local), phase 2 |
| Graph toggle: d3-force canvas, scope all/world/area, kind filter, find | `graph.js` | `/api/graph?scope` (`graph_projection`, `state: building`) | Graph, phase 2 (`kit.GraphCanvas`) |
| Timeline / open loops | `renderOpenLoops`, `appendOpenLoops` | `/api/loops` (`brain_loops`) | Loops page + Overview card, phase 1 |
| Capture, New chat, Tasks, Resume, Open note, Assign note | toolbar `data-action`, `apiAction` | `POST /api/action` -> `brain-control action <name>` (allow-list) | Same allow-list through the adapter; Super+N capture unchanged, phase 1 |
| External navigation (open a note from launcher/dashboard) | `navigation()` 1 s poll | `/api/navigation` (token, path, graph revision) | Pushed `navigate`/`revision` events, phase 1 |
| Refresh knowledge, index status, connection state | rail footer, `setConnectionState` | `/api/summary?retry=1`, `/api/health`, vault watcher | Rail status line from pushed `revision` events, phase 1 |
| Settings: Ask on/off, theme follows desktop | `#ask-disabled`, `theme()` | `/api/ask/settings`, `desktop_theme()`/`theme_css()` | Ask toggle in the Ask panel; theme is the shell palette, phase 1 |
| Auth handoff, browser profile, readiness proof | `brain_auth.rs`, `brain_launch.rs`, `/auth-complete`, `/auth-close.js` | n/a | Deleted in phase 3 (no browser) |
| i18n (`brain.ui.*`, 184 ids) | `i18n.js`, `i18n-data.js`, `translation-engine.js` | `nacre/shell-tools/i18n/en.json` | Same ids via `ui.tr`; JS engine deleted in phase 3 |
| Outside the web app: Settings Brain add-on row, dashboard Brain card (`brain.recent`) | `settings/pages.rs`, `dashboard.rs` | `brain.open`, `brain.recent` | Unchanged; their "open" deep-links into the app |

## 2. App shape

- **Window.** A seventh Bead app in `nacre-shell` (`app::APPS` gains `"brain"` and joins the
  window apps that take `bead.window open`; a second open deep-links the running window, the
  existing single-instance rule). `app_id = "nacre-brain"` matches the existing desktop entry, icon
  and window rules. Default/minimum size are stores in `shell/brain/behavior.luau` and the last size
  is restored, exactly like the Widgets app (`widgets.rs` `window.min_width`, window state file).
  It is an ordinary toplevel, so pin (Super+P), hide (Super+H), scratchpad (Super+S), move/resize and
  workspace rules are the compositor's (N0114) and need nothing Brain-specific; there is no docked
  panel form to detach from.
- **Launch.** `nacrectl ui nacre brain [note-id]` (N0048 route). Daemon `brain.open` (Super+B,
  launcher row, Settings row, desktop entry `Exec=nacre-brain open`) calls that route instead of
  `brain-control action brain`, through the N0216 app scope. Super+N capture stays as it is.
- **Views.** `shell/brain/{main,behavior,graph}.luau` on kit only: `Button`, `IconButton`,
  `TabStrip`, `SegmentedControl`, `Card`, `List`/`ListRow`, `VirtualGrid` (2,000-row lists under the
  8,192 view-node cap), `TextInput`, `Paragraph`, `ChipRow`, `Tooltip`, `Popover`, `GraphCanvas`.
  Sizes, colours, fonts and spacing from `kit.tokens` and palette roles; worlds take palette roles
  by world order, no literals.
- **Rust = data adapter only.** N0024 deliberately keeps the Brain index out of the resident daemon,
  so the adapter talks to the same on-demand worker, not to new daemon routes:
  `crates/nacre-shell/src/brain.rs` opens one connection per window to a Unix socket the worker
  also listens on (`$XDG_RUNTIME_DIR/nacre/brain.sock`, 0600, SO_PEERCRED uid check, bounded JSON
  lines `{op, params}`), starting the worker with the existing `ensure_server` path when absent.
  Ops map 1:1 to the `/api/*` paths `route_checked` serves today and reuse their handlers (`brain`,
  `summary`, `wiki`, `wiki.page`, `note`, `search`, `graph`, `loops`, `ask`, `ask.settings`,
  `action`); the worker pushes `revision` and
  `navigate` events on the same connection, replacing the 1 s poll. An open connection counts as
  activity; the worker still exits 300 s after the last window closes. No duplicate indexing:
  `brain_search`, `brain_registry`, `brain_wiki`, the vault watcher and the graph build stay in the
  worker. The adapter projects `brain.*` stores and handles one `brain.do` action; it holds no
  index.
- **Navigation.** Keyboard first: `/` or Ctrl K focuses search from anywhere; Enter opens the
  selected result or node; Esc backs out one level (Ask > results > screen); Alt+1..5 switch
  screens; Alt+Left/Backspace is history back; in the graph, arrows walk neighbours, Space pins,
  F fits. Every note link is `on_activate` -> `brain.do open <id>`, so reader, search, backlinks and
  graph share one route.
- **New general primitives** (Bead, one line each): `kit.GraphCanvas` (B0232); a read-only session
  store `bead.renderer` = `"gpu"`/`"cpu"` that flips on device loss (general: any app can pick a
  lighter view). Both are general, so no ADR 0004 Generalization entry is needed.

**Directions** (rendered, section 6):

- **A "Library"**: persistent 232 px rail (Overview, Graph, Wiki, Search, Loops; worlds with counts;
  index status), toolbar with search, Ask, Capture, New; content on the right. The reader keeps a
  340 px inspector (local graph, backlinks, links out). Closest to today's web app and to Settings.
- **B "Canvas"**: the graph is the floor of the app. Top bar with brand, `TabStrip`, search, Ask,
  Capture. Overview and Search float as cards over the dimmed full graph; the reader is a 560 px
  drawer beside the live neighbourhood graph. Matches "a gigantic internet of nodes" most literally,
  but keeps a full-graph canvas alive behind every screen.
- **Recommendation: A's rail and plain pages, B's graph screens.** Rail + toolbar everywhere;
  Overview and Search stay plain, readable pages (in the renders B's dimmed graph behind the cards
  costs contrast, and it would keep a full-graph canvas alive on every screen); Graph fills the
  content area beside the rail with B's zoom / Full-Local HUD (the rail's worlds list is the world
  filter and legend); the reader uses B's layout (neighbourhood graph beside a reader drawer) when
  the window is wider than 1,400 px and A's inspector below that. Components are shared, so the mix
  costs nothing extra.

## 3. Graph design (against B0232)

Data: nodes = notes (`id` = note id, `label` = title, `group` = world, `weight` = degree), edges
from `graph_projection` with `kind` in {wiki, mention, related}. Worlds and areas are filters and
the legend, not nodes. Full graph ~2,000/10,000; local graph = `neighborhood{id, depth}`.

| B0232 requirement | Brain use |
|---|---|
| 1 data/style/incremental | `setData` on each pushed `revision`; renames keep positions; style slots = world palette roles, edge alpha by kind |
| 2 highlight/filter/local | `hiddenGroups` = unchecked worlds + archive; `highlight` = search hits and the hovered note's neighbours; `neighborhood{id,1\|2}` for the reader and the CPU fallback |
| 3 focus/selection/pin | `selected` follows the open note; `focus(id)` on "Show in graph"; drag pins, pins persist with the layout |
| 4 events/keyboard | `onSelect` previews in the inspector/drawer, `onActivate` opens the reader, `onContextMenu` -> Open/Pin/Hide world/Copy link, `onHover` drives the tooltip; arrows, Space, Esc as in Navigation |
| 5 labels | Priority policy below, all thresholds as Luau tokens in `shell/brain/graph.luau` |
| 6 reopen/layout | `setLayout()` from the saved file before first paint; `getLayout()` saved as below |
| 7 scale/idle | Full graph only on the GPU renderer; CPU opens local; canvas idles at zero when unchanged |

**GPU/CPU degradation.** B0232 phase 1: no CPU path reaches 8.33 ms for 2k/10k (CPU retained
vector + LOD: 186 ms/frame panning at 2880x1800, 57 ms at 1280x800, 52 ms at zoom 0.4), while
retained GPU vectors take 1.5-11 ms/frame at 2880x1800 on the RTX 4070 and the AMD 7600 iGPU; the
current Vello raster path rebakes on zoom/hover (0.45-1.9 s), so Bead phase 2 builds a dedicated
retained GPU graph path and validates 120 Hz there. The Brain app therefore:

- `bead.renderer == "gpu"`: Graph opens on the full graph with the saved layout; HUD switch Full /
  Local.
- `bead.renderer == "cpu"` (or GPU lost at runtime): Graph opens on the **local graph** of the open
  or last-opened note (depth 1, capped at 150 nodes by weight) with a persistent banner
  "Simplified view: no GPU renderer, so this is the neighbourhood of ..." and an explicit **Open
  full graph** button. The full graph on CPU is a labelled simplified mode, never the default (B0232
  LOD still costs 52-186 ms/frame): culling, every fourth edge at lower alpha, no edges while the
  camera moves, zoom in steps, no layout animation, and the banner stays. Never silent jank. The
  150-node CPU frame time is a phase 2 acceptance number (B0232 has no small-graph CPU row; it must
  stay under 8.33 ms at 2880x1800).
- The reader's embedded local graph is always a neighbourhood, small on both renderers.

**Label priority (hard cap).** Bead's retained text cache admits 1,024 entries per window and
rejects the frame beyond that (B0232: 2,000 labels rejected 7/7), and the window's chrome, reader
and lists use the same cache (roughly 100 text runs per screen in the mock views, more for a long
note). The canvas therefore shows at most **256 labels per frame**, leaving the rest of the window
over 700 entries of headroom, filled in this order until the cap:

1. the hovered node (full text) and the selected node;
2. pinned nodes and the selected node's direct neighbours, by weight;
3. highlight hits (search), nearest the viewport centre first, up to 64; hits beyond that keep a
   ring without a label and the search panel says "+N more hits not labelled", never silently;
4. top-N by weight inside the viewport: N = 24 below zoom 0.6, 64 below 1.2, 128 above;
5. zoom-threshold labels: at zoom >= 1.6 every visible node, by weight, up to the cap.

Labels other than (1) truncate at 24 characters with an ellipsis; a label whose box overlaps a
higher-priority one is skipped (the hover tooltip reserves its box first); zoom thresholds have 0.1
hysteresis so labels do not flicker. The renders apply the same order, truncation and overlap rule
with a 40-label stand-in cap.

**Layout persistence.** Settle is about 1.3 s off-thread for 2k/10k, then the worker parks at zero
CPU. The app saves `getLayout()` (positions + pins by note id, about 0.27 MiB per B0232) to
`~/.local/state/nacre/brain/graph-layout.json` (the existing Brain state dir, `brain_web::state()`;
the app is that file's only writer: temp file + rename, mode 0600) on window close and at most every
30 s after a drag or pin. On open, `setLayout()` runs before the first paint, so the saved picture
appears at once; new or missing notes start next to a neighbour and settle locally. A layout whose
node set differs by more than 25 % from the current graph is discarded and relaid.

**Memory.** Always-on cost is zero: the window and the worker are on-demand and both exit when
unused. While open, the canvas uses B0232's proposed 1 MiB retained graph state plus one transient
copy of the layout payload while saving (about 0.3 MiB). Budget for the whole window: set when B0232
phase 2 measures the canvas (open item; the current Chromium window is the comparison).

## 4. Migration path web -> Bead

| Phase | Lands | Stays in the helper | Deleted at the end of the phase |
|---|---|---|---|
| 1 Native reader, search, overview, loops | `nacre-shell` brain app, `brain.rs` adapter, worker Unix socket + push events, Luau views, goldens at scales 1/1.5, `brain.open`/Super+B/launcher/desktop entry switched to the app | all Brain data modules, the worker, the HTTP server (still serves the web UI) | nothing; `nacre-brain open --web` keeps the browser path, and the app's Graph tab opens the web graph until phase 2 |
| 2 Graph | `kit.GraphCanvas` after B0232 phase 2 validates 120 Hz on real GPUs; Graph screen, reader local graph, layout persistence, label policy, CPU fallback | `graph_projection`, graph build, watcher (via the socket) | `brain/web/graph.js`, `vendor/d3-*`, `renderMap`/`sharedSubjects` and the `/api/connections` UI path |
| 3 Retire the web UI | owner signs off the section 1 parity checklist | `brain-control` CLI and worker (socket only), data modules, `brain_ask`, `brain_git`, curation/migration tools | `brain/web/*`, `brain_http.rs`, `brain_auth.rs`, the browser parts of `brain_launch.rs` (profile, handoff, readiness, migration), the TCP listener and `/api/*` routing in `brain_web.rs`, auth window rules in `config/hypr/conf/brain.lua`, `--web`, unused `brain.ui.*` ids |

Temporary dual path: the web UI and `--web` are deleted 14 days after phase 2 is trusted on the
owner's laptop (standing dual-path rule); expiry tracked here.

Risks: B0232 slips (phase 1 is useful without the graph; the web graph covers it); the kit has no
rich text, so the reader renders Markdown as headings, paragraphs, lists and links (code blocks and
images are a listed gap); the worker socket is a new local trust boundary (0600, peer uid, bounded
frames, the same handler validation as HTTP); Ask keeps its explicit opt-in before any Claude call;
the semantic search worker stays optional and off by default.

## 5. Open questions for the owner

1. Direction: A (rail), B (canvas floor), or the recommended mix (A's rail and pages, B's graph)?
2. Without a GPU, is "open on the local graph with a banner, full graph one click away" right, or
   should the full graph stay the default at reduced quality?
3. Drop the old Connections view (world cards, confidence) in phase 2, or keep it as a page?
4. Ask the Brain: keep the opt-in toggle in the app's Ask panel, or move it to Settings only?
5. Should the graph include worlds/areas as hub nodes (Obsidian "folders") or stay notes-only with
   worlds as colour and filter (designed and rendered: notes-only)?

## 6. Evidence (design only)

- Renders: `.lead/renders/n0217-{A,B}-{overview,graph,local,note,search}-{1,1.5}.png` (20 files,
  1280x800 logical, dark palette) from one dev-1 container with the bead-test harness. Synthetic
  fixture only: 2,000 seeded notes in 8 worlds / 33 areas, 10,000 preferential-attachment links
  (wiki 60 %, mention 30 %, related 10 %), a numpy Fruchterman-Reingold layout settled offline; the
  full graph is a pre-rendered tile standing in for `kit.GraphCanvas`, the local graph is drawn with
  `ui.Polyline`/`ui.Box`. Every
  PNG was reviewed at both scales (`.lead/renders/README.md`).
- Nothing is integrated: the mock views (`shell/brain-n0217/`), the render test and the fixture
  script stay untracked in the worktree; only this spec is committed.
