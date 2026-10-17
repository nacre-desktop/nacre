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
| calculator | Unchanged calculator.js at `54a9e5d6`, actual offscreen Quickshell 0.3.2 | 217 |
| calculator-extra | Same source at `1e2d6755`, separately frozen numeric and whitespace edges | 33 |
| calculator-powers | Same source/runtime, separately frozen finite/nonfinite power matrix | 63 |

Provider references execute JavaScript in Node, not a QML lifecycle. They preserve
complete rows and optional-field presence; English fallback translation, time and
MD5 are controlled. The desktop fixture includes the exact desktop-entry files.
The developer capture scripts in `tools/tests/launcher-reference` refuse to
overwrite output. Config cases and default constants were captured separately
from the same source revision. `data/defaults.json` contains the existing 40 emoji
and software categories; `base-menu.json` is the unchanged built-in input.

Calculator outputs were frozen before the Rust port, using private HOME/XDG and
D-Bus. Their original JSON bytes are preserved, including source hashes and the
runtime version. Arithmetic runs in-process; only lowercase whole-word `to`/`in`
queries use the existing conversion worker. The existing QML implementation stays
as the fallback for one release. Fourteen-digit formatting retains decimal tie
rounding, subnormal values and the original exponent thresholds.

Intended differences: malformed JSON diagnostics use serde's wording; absolute
XDG roots follow NacrePaths; oversized or cyclic application trees fail visibly.
If an error preview's 12 UTF-16-unit boundary splits a surrogate pair, its final
unit becomes U+FFFD so the private link retains valid UTF-8.
Semantic extension errors and fallback behavior remain exact. Corrupt on-disk
preferences are rejected by the daemon writer, retaining its cached state. The desktop parser
retains current Quickshell catalog behavior, including field-code removal,
Hidden shadowing and literal argument tokenization; it never invokes a shell.

Desktop locale/Exec code adapts [Quickshell v0.3.2](https://github.com/quickshell-mirror/quickshell/blob/v0.3.2/src/core/desktopentry.cpp); its notice and LGPL text remain in `nacre/shell-tools/launcher-engine-NOTICE.txt`.
