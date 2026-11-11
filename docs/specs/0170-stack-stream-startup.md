# N0170: Keep stack data through compositor startup

## Diagnosis and scope

The missing N0164 parent segment follows an empty daemon `collections` list;
the widget deliberately hides it in that case. Read-only inspection after boot
on compositor `1028c5b9` found two collections, valid boolean Nacre capabilities,
and initial `CollectionsChanged` snapshots from both collection-enabled streams.
At the same time, the daemon published no collections and disabled
`workspaceCollections`, while publishing other native capabilities normally.
Window-focus patches continued on the daemon topic, so the source was active.
The r3073 to r3079 capability addition (`launch_workspace: true`) is compatible
with Nacre's additive boolean parser. No protocol version change is needed.

The daemon and standalone observer treat a bounded capability probe as durable
topology authority in two places. An initial failed 500 ms probe defaults to
false and selects a stream without collection events; later `ConfigLoaded`
probes never upgrade that stream. A failed reload probe also deletes collection
state already received, which a later successful probe does not restore.
Both paths can leave collections absent despite a healthy compositor. The boot
journal does not record probe replies, so it cannot distinguish the exact boot
trigger. Output reconnects cannot fix a missing collection subscription.

Negotiate collection events independently of that initial probe, using the
existing acknowledged request/fallback mechanism. Keep legacy hover and stock
event fallback. Continue gating published features on capabilities, but retain
the latest received topology privately while a probe disables publication, so
the next successful reload restores it without another topology event. This
changes neither the stack UI nor compositor IPC. No new service, polling,
dependency, temporary duplicate or always-on memory budget is introduced.

## Acceptance and evidence

Regression coverage must recover nonempty collection data for two outputs when
the initial capability probe is false and a subsequent configuration probe is
true, restore received rows after a false then true reload probe without a new
collection event, and retain legacy stream negotiation. Run the complete
`desktop` job from
`.github/workflows/check.yml` in a private Arch container on dev-1 (8 CPUs,
16 GiB, Cargo `-j2`). Raw evidence stays outside Git. The desktop TEST host is
unavailable; no nested captures or live changes are authorized or required.

The three focused collection-stream tests pass. Restoring either original
production defect separately makes the new regression fail: the initial probe
selects the wrong stream, or the reload probe loses the received rows. The daemon
integration suite passes 24 tests with one intentional ignore. Stock-fixture
connection totals now include the one rejected collection-stream probe per
daemon start; exact shared-stream and reconnect checks remain in place.

The complete desktop job passed with exit 0 on dev-1 in the required Arch
container, including a full rerun after rebasing onto `e85797656`:

- Formatting, Clippy, examples, workspace tests, the minimal protocol consumer,
  release helper, trace tests, feature checks and all product binary builds passed.
  The helper library passed 1,004 tests (three ignored); the shell passed 871
  tests (four ignored). No check limit was relaxed.
- Luau/schema/wallpaper checks, dependency advisories/bans/licenses/sources,
  npm, translation coverage, generated translations and `tools/check.py` passed.
- The native AUR gallery and the separately enabled CPU raster-memory test passed.

Product and regression-test files match the remotely validated sources. No
goldens changed. Deploying the corrected Nacre helper and restarting its daemon
will be necessary; no compositor package or relog is required. Live deployment
and owner acceptance remain with the lead. Product delta: +13 / -20 lines,
net -7; no product files removed.
