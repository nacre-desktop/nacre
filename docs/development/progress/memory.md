# Shell memory and launcher progress

## Done

- Isolated branch `perf/shell-memory` based on main `5483399`.
- Inspected lifecycle and image paths; baseline RSS figure was warmed, not fresh.
- Imported the measurement tool/documentation from open PR #4; helper migration
  is outside this branch. Extending rollup accounting before optimizations.

## Next

- Test PSS/USS accounting and record fresh/panel/static/video/output scenarios in
  a disposable nested compositor and HOME.
- Attribute memory and launcher time; implement measured lifecycle/media wins.
- Compare before/after, run source/native tests and CI, rebase and merge one PR.

## Open questions

- How much of ~555 MiB RSS is private versus shared library/renderer memory?
- What is the memory/latency tradeoff of a short launcher reuse window?
- Does lazy video file loading avoid Qt Multimedia on a fresh static shell?

No live settings or personal preferences have been changed.
