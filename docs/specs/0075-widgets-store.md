# N0075 — Opaque Bead widget layout store

## Contract

Nacre registers the writable `widgets` settings store at
`$XDG_CONFIG_HOME/nacre/widgets.json`. The store contains one setting,
`layout`, whose default is `null`. Bead owns the document format; the daemon
stores it without interpreting its contents. The persisted value must be JSON
`null` or an object and its serialized representation is limited to 256 KiB.

Clients subscribe to `settings.store.widgets` and write with
`settings.set {store: "widgets", key: ["layout"], value, rev}`. Writes use the
standard settings revision check, dirty publication, atomic persistence and
0600 file mode. A stale revision is rejected. Revisions are runtime counters
and restart at 0 when the daemon starts; the layout data persists across restart.

## Implementation and evidence

Registration is in `nacre/shell-tools/nacre-state-files.json`; the precise
`widgets.layout` write gate and document bound are in the daemon settings
validator. The protocol fixture records null default, revision 0 → 1, topic
publication and stale-revision rejection. Daemon coverage checks get/set,
conflict, oversize rejection, persistence after service reload and file mode.

The daemon does not validate widget instances, hosts, package identifiers or
layout version fields. That validation belongs to Bead.
