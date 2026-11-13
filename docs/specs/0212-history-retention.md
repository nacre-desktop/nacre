# N0212: History retention

Four Privacy settings control notification, clipboard, screenshot, and Nacre AI chat retention in days; `0` keeps everything. Notification and clipboard keep their existing storage paths. Screenshot cleanup only considers capture files recorded by Nacre, verifies the recorded file identity, and moves them to desktop Trash through `gio trash`. AI cleanup is confined to UUID directories under Nacre's private local chat root, moves old folders and their files to Trash, and retains saved, pinned, starred, and open chats. Clipboard pins survive age-based cleanup.

Notification retention is read when the notification daemon starts, so changing it requires restarting the daemon. Clipboard retention is read during clipboard pruning. Screenshot and AI cleanup run once per day through the existing daemon behavior runner. Screenshot and AI defaults are `0` so existing installs retain their data until users opt into deletion.

Dev-1 follow-up validation: `cargo fmt --all --check`, helper clippy, 11 focused retention tests, and both Settings registry/translation rendering tests passed. Full `nacre-helper --lib`: 1,038 passed, 3 ignored. Temporary-HOME coverage includes age boundaries, pins, zero retention, screenshot identity/Trash, safe roots, AI chat activity, cap/depth fail-closed scans, open tmux session preservation, trash failure, and once-daily cleanup.
