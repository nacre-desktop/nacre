# Launcher engine

GPL-3.0-only. Ranking and provider logic ported from Nacre's JavaScript at
`83a9a9a493dcdc53f8fe42c94f01ac2553694035`. No UI or transport dependency.

The immutable gzip fixtures contain the original JSON bytes, source hashes,
synthetic inputs and expected outputs. Tests verify their uncompressed SHA-256
before comparison. Never regenerate expected outputs to accommodate a port.

| Fixture | Reference | Cases |
| --- | --- | --- |
| javascript | Node 22, en-US, clock 1720000000; four original JS modules | 394 |
| providers | Extracted original QML functions; synthetic outside services | 544 |
| config | Original menu-model.js validation and extension merge | 18 |
| legacy | Current DesktopActions list plus app-browser.js, default/disabled settings | 2 |
| emoji | Existing native helper, immutable output from 31 queries before caching | 31 |
| desktop-catalog | Offscreen Quickshell 0.3.2, synthetic application folders, en/tr locales | 2 |

Provider references execute JavaScript in Node, not a QML lifecycle. They preserve
complete rows and optional-field presence; English fallback translation, time and
MD5 are controlled. The desktop fixture includes the exact desktop-entry files.
The developer capture scripts in `tools/tests/launcher-reference` refuse to
overwrite output. Config cases and default constants were captured separately
from the same source revision. `data/defaults.json` contains the existing 40 emoji
and software categories; `base-menu.json` is the unchanged built-in input.

Intended differences: malformed JSON diagnostics use serde's wording; absolute
XDG roots follow NacrePaths; oversized or cyclic application trees fail visibly.
Semantic extension errors and fallback behavior remain exact. Corrupt on-disk
preferences are rejected by the daemon writer, retaining its cached state. The desktop parser
retains current Quickshell catalog behavior, including field-code removal,
Hidden shadowing and literal argument tokenization; it never invokes a shell.

Desktop locale/Exec code adapts [Quickshell v0.3.2](https://github.com/quickshell-mirror/quickshell/blob/v0.3.2/src/core/desktopentry.cpp); its notice and LGPL text remain in `nacre/shell-tools/launcher-engine-NOTICE.txt`.
