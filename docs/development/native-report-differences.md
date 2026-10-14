# Native support-report boundaries

The library matches all 26 frozen synthetic collection/rendering cases. Production
queries use D-Bus/socket APIs for systemd, NetworkManager and running compositor
state; no Python, nmcli, hyprctl or per-action niri process is started.

Intended difference: the retained Python-version compatibility field is unavailable;
the report does not start an interpreter just to obtain that obsolete runtime fact.
Intended difference: required-command checks omit replaced IPC frontends and Python;
the remaining package requirements stay until their migration waves are complete.
Intended difference: niri's running version comes directly from IPC; the installed
binary version remains in its existing separate field, rather than repeating it.
Intended difference: failed-unit rows use stable single-space state columns rather
than systemctl's terminal-dependent padding, preserving the same unit/state/details.
Intended difference: release disk usage reads filesystem allocation blocks directly,
counting hard links once and following no symlinks, within the existing deadline.
Intended difference: unexpected query/parse failures reach stderr; report fields
retain their existing unavailable/partial behavior and never include raw tool stderr.

Network profile names use `GetSettings`, which excludes secrets; `GetSecrets` and
Wi-Fi scans are never requested. See the
[NetworkManager contract](https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Settings.Connection.html).
Names are used only at the existing export-redaction boundary. Reply sizes and
whole-query deadlines are bounded. Private tests exercise socket/bus traffic,
secret/scan refusal, oversized replies and operation without PATH tools.

The existing niri configuration validator and installed compositor version commands
remain the actual tools, alongside journalctl, pacman, pw-dump, Qt version tools,
lspci and modinfo. No owner session, configuration, packages or services are changed
by migration validation.
