# Helper parity contracts

Frozen black-box references cover all 94 Python entrypoints in the requested
shell-tools, bin, and brain inventory, plus module/CLI boundaries. They complement
all existing tests in `nacre/shell-tools/tests`, `tools/tests`, `ai/tests`, and
`brain/tests`. [contracts.json](contracts.json) maps helpers to cases and existing
specs; [FORMAT.md](FORMAT.md) describes fixtures, isolation, and coverage limits.

On Linux, install developer-only Bubblewrap, `dbus-daemon`, Pillow and
`dbus-next==0.2.3` in the checking environment. Build the pinned Orient binary:

```sh
export NACRE_ORIENT_BINARY="$PWD/deps/orient/target/release/orient"
python tests/parity/helpers/run.py
python tests/parity/helpers/run.py --case 'helper-service/*'
python tests/parity/helpers/run.py --case 'menu-backend/*' \
  --command 'nacre-helper {name}' --traces /tmp/helper-parity-traces
```

The default command is `python3 nacre/shell-tools/{name}.py`; cases override it for
other entrypoints. Replacement templates are split into argv without a shell.
The executable is resolved on the invoking PATH and mounted into the sandbox.
For a replacement linked directly to pinned Orient core/decoder, add
`--orient-direct`: known computational RPC transport observations may differ,
while engine/version, palette/source/options, files/modes, decoder commands, and
all desktop actions remain exact. Original RPC observations remain frozen.

To add a case, create `HELPER/case-NAME.json` with public fixture data, for example:

```json
{
  "helper": "menu-backend",
  "args": ["convert", "1 km to m"],
  "commands": {"qalc": [{"stdout": "1000 m\n"}]}
}
```

Then explicitly record the Python reference, review it, and replay it:

```sh
python tests/parity/helpers/run.py --record --case 'HELPER/NAME'
python tests/parity/helpers/run.py --case 'HELPER/NAME'
```

`expected-NAME.json` compares parsed JSON stdout (or explicit text/JSON-lines),
exit status, filesystem contents/modes/deletions, argv/stdin and compositor wire
messages. Stable stderr may be checked explicitly; other stderr goes to traces.
Only declared timestamps, temporary paths and PIDs are normalized, with a reason
beside each rule. IDs, cache hashes, counts, versions and packet order remain exact.

Every case gets a disposable HOME, fake kernel files, masked application
executables, private compositor/bus sockets and PID/network/IPC namespaces.
Unscripted tools fail; there is no live-desktop or internet fallback. Typed D-Bus
rules record equivalent actions and reject unexpected calls. Import-only modules
use a child-process operation adapter; see FORMAT.md for the replacement boundary.

UUID/credential creation, device/inode-derived cache identities, real graphical
sessions/hardware, remote services and model inference remain documented limits
with existing tests retained. Palette/CLI references were captured only after the
Rust Orient integration landed on main. The pin and revision are recorded in the
coverage manifest. Run `tools/check.py` for harness, parity and existing checks.

The 2026-10-13 niri parity change refreshes only compositor capability/event
references and the control-tools unavailable reason. It adds explicit limits,
managed desktop/output/controller support and editor-compatible mode strings.
Wire requests, IDs and other helper observations stay frozen; the prior references
remain in Git history. Desktop-settings retains its structured-error exit convention.
