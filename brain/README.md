# Nacre Brain

Optional local knowledge browser for your Markdown vault. Install with
`./install.sh --with brain --apply`, then open Nacre Brain from the launcher.
Your existing vault stays at its configured location; installing the application
does not create a new populated memory store or copy accounts. See
[optional apps](../docs/optional-apps.md) for setup and [recovery](../docs/recovery.md)
for updates. The local semantic model is optional and separately installed.

Brain opens when you choose it from the launcher or use its shortcut. Closing the
window keeps it closed; nothing stays resident.
The loading view uses your current desktop colors immediately, before the
knowledge index finishes loading.

The loopback HTTP worker now runs in `nacre-helper`. A standalone
`nacre-helper brain-control serve` exits after five idle minutes. The Nacre
daemon starts it on demand (`nacrectl brain open|capture`, the `brain` topic);
without a daemon the launcher starts it directly. When updating from the Python server, reload an already-open Brain
page once after its authentication
handoff so it loads the origin-bound API credential hook.
