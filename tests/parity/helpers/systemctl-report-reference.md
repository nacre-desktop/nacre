# Private systemctl output reference

`crates/nacre-helper/tests/fixtures/systemctl-report-reference.json` records ten
cases from the real `systemctl` executable and identifies its version. Each call
runs inside the existing Bubblewrap harness against a private strict fake
systemd manager. Only `Properties.GetAll` reads are accepted. The recorded wire
calls prove that no owner bus, unit action or unlisted method was used.

The test process uses `SYSTEMCTL_FORCE_BUS=1` and `SYSTEMD_OFFLINE=0`, documented
in [systemd's test environment interface](https://systemd.io/ENVIRONMENT/), to query
the explicitly supplied private broker despite the sandbox's chroot detection.
These process-local settings do not alter the owner environment. The harness
still masks system directories, clears inherited endpoints and isolates the
network. Missing-procfs warnings are retained as captured stderr evidence and
are separate from the report stdout contract.

Measured behavior:

- Property output follows the insertion order of the `a{sv}` GetAll reply, rather
  than the order of repeated `-p` options. Filtering preserves reply order.
- Empty string properties print `Key=`. Signed status numbers print in decimal.
- Unit groups have one blank separator; successful output ends with a newline.
- GetAll uses an empty interface argument, so Unit and Service fields are read
  together. Unit paths are computed directly; no GetUnit/LoadUnit calls occur.
- A GetAll error stops further unit queries, exits 1 and retains earlier stdout.
  A missing first unit leaves no stdout or queries for later units.
- A reply containing `LoadState=not-found` is distinct from a GetAll error: its
  properties print and the command exits 0.

The fixture is never overwritten by its capture script. Replay checks stdout,
exit status and exact read-only wire calls. Any native implementation must retain
these strings and partial-failure behavior; existing report goldens are unchanged.

The developer control also runs `systemd-report-parity` against every captured
reply and compares exact stdout, returned exit result and wire calls. Build the
example from the revision under test and set `NACRE_SYSTEMD_REPORT_PARITY` to its
exact executable path. Missing artifacts fail the check; no test is skipped and
no live bus fallback is permitted. The example itself returns a JSON comparison
result, so its process exits 0 even when the returned systemctl-equivalent result
is 1.
