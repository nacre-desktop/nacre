# N0138 — Brain taxonomy and writer validation

Implements overhaul U2. Registry names, aliases and patterns start from the owner’s
seed vocabulary; explicit valid names grow the taxonomy under the vault lock.
Seed entries retain their original spellings and status. No live migration or
installation is included. Viewer routes, UI and git internals are outside scope.

## Storage and writer contract

`brain/registry-seed.json` is embedded in the helper. Missing `registry.json` is
seeded on the first categorized write. The flat registry has `worlds` and `areas`
arrays: name, aliases, patterns, status, created/updated dates, area parent and
proposal use dates. Status supports seed, auto, proposed and merged-into; future
maintenance owns merge/split/retirement. Writes atomically replace the registry
while holding the existing vault lock and enqueue it through `brain_git`.

Both note and memory commands accept repeated world, area and topic options.
Names and aliases resolve before fuzzy deduplication; an area passed as a world
implies its parent. New explicit worlds and areas are immediately auto. A clear,
unmatched title subject before a colon proposes an area under the best world.
Three assigned notes spanning two calendar dates promote a proposal, including
subsequent notes without a colon title; tests inject the date. Proposed areas
remain low-confidence until promotion.
Repeated aliases count once as classification evidence, and short aliases are
case-sensitive for mentions. Generic body words cannot add a secondary area.
Ordinary prose never supplies arbitrary new category names. Missing or weak
categories use registry patterns and aliases; secondary areas require stronger
evidence than primary classification. Unsorted is a review flag, never a world.

New names have 1–4 words and at most 40 characters after Title Case normalization.
Known entity aliases such as Red v1 still resolve; they cannot create versioned
category names. Paths, URLs, hashes, full or partial dates,
versions and metadata labels are refused. Topics use lowercase ASCII hyphens.
Headers contain only nonempty category fields. The shared metadata parser stops
at the header boundary; a blank after the title remains supported for existing notes.
Memory JSON retains path/worlds/topics and adds areas/review. Feedback goes to
stderr so stdout paths and JSON remain machine readable. Note writes no longer
create wiki pages. Archive and digests are recognized note kinds.

## Compatibility and deployment

The Python memory reference delegates to the native helper, including its imported
`remember()` API. Its CLI remains a native exec trampoline. This avoids a second
taxonomy implementation. Delete the rollback reference 7 days after the native
replacement is trusted; retain parity fixtures until then. No installer ran in
this unit. The lead must install the built native helper and atomically replace
the pinned agents’ standalone memory script; exact commands are in `.lead/report.md`.

The public-name regression guard permits the owner identifier only in the
explicitly requested `brain/registry-seed.json` vocabulary data; runtime names
and the other forbidden identifier remain guarded.

The simplicity review removed duplicate fuzzy searches after resolution and the
Python category writer. The native module includes validation and growth that the
dry-run classifier did not provide; there is one shared writer resolver.

## Validation

Synthetic fixtures only: all seed aliases, invalid labels, explicit growth,
proposal promotion with an injected date, near duplicates, header boundaries,
process concurrency and Python/native parity. Full desktop CI is reproduced in
the task’s bounded Arch container on dev-1. Results are recorded in the report.

## Remaining integration boundaries

The native viewer must delegate `fields()` to the shared header parser. That
function is in `brain_web.rs`, explicitly excluded from this unit while another
unit edits it; the parser-only exception is awaiting the owner’s answer. The
discovery API continues to respect caller-supplied annotations. It must not
replace them from prose. Optional discovery-vector assistance is not connected;
writers currently use registry patterns and entity/alias mentions.
