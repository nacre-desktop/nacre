# Menu CLI reference capture

The 62 `menu-backend/cli-*` cases were captured from the unchanged Python
`menu-backend.py` at Nacre `488fa6d1`, before its removal. SHA-256:
`d74ca0b2a6d6d41a173b399fa5fbf876472d9ef8b18572d604ea8b20108eb958`.

Each of the 16 actions has an omitted-value and supplied-value case. Additional
cases cover help ordering, missing/invalid actions, extra arguments, option
placement and abbreviation, repeated flags, explicit flag values, negative
numbers, hyphenated values after `--`, and empty/whitespace queries.

All execution used the existing Bubblewrap harness with disposable HOME,
private bus addresses and fixture-only executables. Package operations use a
scripted manager; package queries are shorter than two characters and never
fetch repositories. Snapshot cases mask only the already-declared creation
timestamp in stdout and its persisted JSON copy. Every other output byte, file
mode, deletion, process effect and IPC effect remains recorded. Stderr is retained
exactly, including argparse diagnostics; no future parser wording is substituted.

The measured exit distribution is 35 successful calls, 16 operation errors and
11 parser errors. Parser errors cause no filesystem, process or IPC effects.
The original expected files remain unchanged. `freeze_menu_cli.py` refuses a
changed/absent source or any existing destination before executing a case; the
standard runner also refuses re-recording migrated helper references.

Re-run the frozen checks against an explicitly built candidate. Any accepted
grammar, help stdout, exit or side-effect mismatch is a regression. A parser
stderr wording difference must be separately declared and reviewed; these
references deliberately preserve the original wording.

Intended exception difference: missing reminder minutes retain the exact
`TypeError` class/message, empty stdout, exit 1 and private lock, while Rust
omits retired Python traceback frames. Only the two declared `{}` inputs use
this equivalence; negative controls reject any broader scope or message change.
All parser diagnostics and help output match the frozen text exactly.

Three additional terminal-action cases freeze journal stdout and exits 0/7,
plus update cancellation before preflight. Native dispatch inherits terminal
streams and status; the owner-selected update command remains unchanged.
Installed helper assets take priority over a checkout in the working directory;
an explicit developer asset override remains available.
