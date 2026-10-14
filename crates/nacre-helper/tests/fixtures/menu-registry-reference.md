# Frozen menu action boundaries

`menu-registry-reference.json` freezes 55 current Python module-action cases
before the Rust port: package manager arguments, web-app validation and files,
reminder bounds/schedules/notification arguments, URL/duration/Unicode edge
cases, cancellation, errors, file
bytes/modes and deletions. Developer mocks provide a fixed UUID and wall clock;
the production helper exposes no test-only UUID/time settings. This supplements
the unchanged black-box CLI fixtures and historical menu tests.

`menu-systemd-reference.json` freezes six actual `systemd-run` translations
through a private mock manager, which cannot start units. The capture uses
`--no-block --quiet` to suppress waiting/display; transient properties remain
the same. It covers the complete timer/service properties, descriptions,
absolute executable arguments and control/Unicode text. The native wire test
compares these exact objects and checks immediate, unrelated, failed and missing
job completion signals. The latter has a total five-second deadline.

Intended difference: reminders use systemd D-Bus directly rather than starting
`systemd-run`/`systemctl`; failed jobs surface their D-Bus reason. Unit names,
properties, schedule, notification arguments and state files remain compatible.
Web-app desktop files publish atomically with mode 0600 rather than a temporary
mode-0644 window before chmod. Do not regenerate references to bless a mismatch.
