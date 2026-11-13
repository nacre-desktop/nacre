# N0212: History retention

Four Privacy settings control notification, clipboard, screenshot, and Nacre AI chat retention in days; `0` keeps everything. Notification and clipboard keep their existing storage paths. Screenshot cleanup only considers capture files recorded by Nacre, verifies the recorded file identity, and moves them to desktop Trash through `gio trash`. AI cleanup is confined to UUID directories under Nacre's private local chat root; saved, pinned, and starred chats are retained. Clipboard pins survive age-based cleanup.

Notification retention is read when the notification daemon starts, so changing it requires restarting the daemon. Clipboard retention is read during clipboard pruning. Screenshot and AI cleanup run once per day through the existing daemon behavior runner. Screenshot and AI defaults are `0` so existing installs retain their data until users opt into deletion.

Dev-1 validation: `cargo fmt --all --check`, helper clippy, 7 retention tests, 3 notification tests, clipboard test, settings golden, and capture-history fixture passed. Full `nacre-helper --lib`: 1,034 passed, 3 ignored. Temporary-HOME coverage includes age boundaries, pins, zero retention, screenshot identity/Trash, safe roots, AI chat activity, and once-daily cleanup.
