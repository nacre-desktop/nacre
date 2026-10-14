# Fixture format and isolation

These frozen Python references are the observable spec for a replacement helper.
They complement **all** existing tests in `nacre/shell-tools/tests`, `tools/tests`,
`ai/tests`, and `brain/tests`; those suites remain required. [contracts.json](contracts.json)
lists each entrypoint, its cases, related existing tests, and known coverage limits.
No production helper is imported by the runner or edited to capture a reference.

Run from the checkout on Linux with Python 3.14, Pillow, Bubblewrap, `dbus-daemon`,
and developer-only `dbus-next==0.2.3` installed in the checking environment:

```sh
export NACRE_ORIENT_BINARY="$PWD/deps/orient/target/release/orient"
python tests/parity/helpers/run.py
python tests/parity/helpers/run.py --case 'helper-service/*'
python tests/parity/helpers/run.py --case 'menu-backend/*' \
  --command 'nacre-helper {name}' --traces /tmp/helper-parity-traces
```

The replacement executable is resolved on the invoking PATH and mounted into the
sandbox. Templates are split into argv; no shell evaluates them. A Rust binary
may need a dispatcher/adapter for the module-operation cases described below.
The pinned native Orient executable is mounted only for `native_engine` cases;
its argv and JSON stdin are recorded through a forwarding shim. Build that pin
first. The harness never updates the deployed runtime.

Each run has a fresh HOME/XDG directories, an empty inherited environment, fake
`/proc`, `/sys`, and `/etc`, an empty `/usr/local`, and private PID/network/IPC
namespaces. The checkout and libraries are read-only. PATH and `/usr/bin` contain
only fixture executables and the Python/shell primitives; absolute application
calls cannot reach live executables. There is no live compositor, bus, display,
or internet fallback. Containers must permit nested user/mount namespaces (the
CI check container needs no added capabilities on Ubuntu 22.04; its outer
seccomp/AppArmor restrictions permit the inner sandbox; see
[CI jobs and owners](../../../docs/development/ci.md)).

To add a case, put `case-NAME.json` in the helper's directory with `helper`, `args`,
optional `stdin`/`stdin_repeat`, `fixtures`, `commands`, `env`, and `sockets`.
Optional `python_version` controls only the interpreter's `--version` response
for runtime diagnostics. All other execution uses the real interpreter with the
same executable path. This fixes a host input; it never normalizes output or
changes the frozen version. Scripted sockets belong to the fixture identity and
have mode 0600, including when the runner starts as root in CI.
Fixture paths are relative to the disposable root: `home/`, `proc/`, `sys/`,
`etc/`, `runtime/`, `tmp/`, `local/`. Text supports `{home}` and `{root}` tokens;
fixtures can set octal `mode`, `mtime`, `base64`, `link`, or `directory`.
Use public synthetic data. Record and inspect the diff explicitly:

```sh
python tests/parity/helpers/run.py --record --case 'HELPER/NAME'
python tests/parity/helpers/run.py --case 'HELPER/NAME'
```

`expected-NAME.json` compares parsed JSON stdout by default, `stdout: "jsonl"`
compares ordered packets, and `stdout: "text"` compares bytes decoded as UTF-8.
Exit code, changed/deleted files, directory/file modes, symlink targets, process
argv/stdin, and compositor messages are compared. Binary data uses base64.
Changes are observed in HOME, temporary files, sysfs fixtures, runtime files, and
`/usr/local`; unchanged fixture bytes are omitted. `compare_stderr: true` opts
into exact stderr comparison; otherwise stderr is retained in requested traces.
Python tracebacks are not part of the replacement stdout/exit contract.

Intended difference: `nacre_shell/native-image` records the decoder-source
fingerprint change at exactly `/stdout/engine` and `/stdout/source/engine`.
Its `engine_provenance` rule requires the declared frozen identity and checks
the candidate against the current dependency lock, adapter pin and compiled
Orient `info` reply. Colors, core identity, source/options, files/modes and
actions stay exact. These frozen outputs cannot be re-recorded; arbitrary
hash/version fields and cache filenames are never normalized by this rule.

The six injected-provider service cases declare `native_service_fixture`.
With `--command` and `--service-fixture`, they run the developer-only Rust
example around the same native `serve` library. Exact provider-source SHA256
checks limit this to the frozen gate/crash/exit/oversized test behaviors; the
Python baseline still runs its original fixtures. Replies, ordering, IDs,
exit codes and all file effects stay exact, with no output normalization.
The native installed runtime uses compiled providers and never loads Python
from the legacy developer `NACRE_DASHBOARD_TOOLS` module override.

A `commands` entry maps a name to rules with exact `argv` or an explicit fallback,
`stdout`/`stdout_base64`, `stderr`, `code`, and optional `writes` (paths can use
`{arg0}`, `{arg1}`, etc.). Set `read_stdin: true` for piped input; inherited service
stdin must not be consumed. Unscripted calls fail. UNIX `sockets` specify a path
and ordered connection replies; exact received messages, including newlines,
are frozen. Unsolicited Hyprland event streams record an empty sent message.
The niri event cases replay the existing shared event-stream fixture.

`dialogue` steps send input, receive an exact number of packets, and release
fixture files. They prove FIFO lanes, independent work, duplicate-active IDs,
64 pending slots, oversize bounds, exceptions, and hosted process exits without
sorting output or relying on response timing.

For modules without a CLI, `api.py` is a **child process** boundary: module/path,
operation, then JSON `args`/`kwargs`. `{"$path":"..."}` means a Path; `construct`
provides class constructor arguments. Paths, sets, ConfigParser and
CompletedProcess values have explicit JSON encodings in that driver. A replacement
adapter must expose this same boundary; the runner itself remains black-box.
An exact one-key `{"$datetime":"ISO 8601 timestamp"}` object supplies a datetime
for deterministic calendar tests, preserving its supplied timezone.

After the Wave 1 cutover, the default registry runs migrated helpers through
`nacre-helper` and module cases through the developer `parity-api` example. Build
the exact revision with `cargo build -p nacre-helper --bins --examples`. Explicit
`NACRE_HELPER_BINARY`, `NACRE_PARITY_API` and `NACRE_SERVICE_FIXTURE` paths are
authoritative; otherwise the installed helper or newest local debug/release
build is selected. Missing native artifacts are errors, with no Python fallback.
Selection by modification time does not prove source identity: rebuild the exact
revision before a gate and set all three executable paths explicitly in CI.
Recording any selection containing migrated helpers is rejected before writing
references. Untouched helpers keep their existing Python boundary. Native
default routes use the same narrow Orient transport equivalence described below.

D-Bus cases start a private bus with a strict fixture peer. Each rule matches the
entire destination/path/interface/member/signature/typed body, supplies a reply,
and declares equivalent recorded actions. For example, disabling Wi-Fi through
NetworkManager Properties.Set must match the `nmcli radio wifi off` action. This
is an explicit, reviewed transport equivalence, not an output normalization.
Unexpected calls become failing side effects. `--traces` retains actual D-Bus
calls as well as comparison results. Harness tests include a direct D-Bus
replacement and a wrong-argument negative control.

Only timestamps, temporary paths, and PIDs may be normalized. Each case declares
an exact JSON pointer (RFC 6901), optional embedded JSON pointer or narrow regex,
and a reason. File-content normalization preserves every other byte. No UUIDs,
cache hashes, counts, versions, event ordering, or general paths are discarded.
Current cases mask dashboard/launcher/session/art/rotation wall-clock fields,
a dated Dolphin backup directory, a temporary video frame filename, and the
localized clock display; their individual reasons are next to the fixtures.
The virtual `/case` and `/repo` paths are fixture inputs, not broad path rewriting.

Limits: full preview/publication cache identities include device/inode/ctime,
random UUIDs and browser credentials cannot be frozen under these rules, and
live hardware, graphical sessions, model downloads/inference and remote services
need their existing tests and dedicated integration checks. The native image
case uses the public `cache_dir` option pointing to a read-only fixture directory,
so its deterministic palette can be checked without persisting metadata hashes.
Version-reporting cases retain the reference interpreter version (3.14.7).
The Rust Orient integration was verified on main before the final palette/CLI
cases were recorded; its pin and reference revision are in contracts.json.

A replacement linked directly to the pinned `orient-core`/`orient-decoder` may
opt in with `--orient-direct` alongside `--command`. Only recognized, typed native
Orient computational RPC observations are treated as the alternate transport;
versions/engine IDs, palette/source/options, files/modes, decoder processes, all
other actions, and unknown/malformed RPC calls remain exact. Frozen references
retain the original RPC argv and stdin. This does not change the default check
or permit recording a replacement. The engine's own core/decoder golden tests
remain required evidence for direct binding; they live in the pinned Orient repo.
