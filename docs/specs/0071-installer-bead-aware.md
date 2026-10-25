# N0071 — Installer validation follows the active shell

Status: Implemented; remote desktop CI pending.

When `nacre/shell-tools/install.py --code-only` begins with `nacre ui-shell status` reporting `active: bead`, it stages the new QML source but does not call the QML desktop IPC probe and does not change `source.good`. After restarting the shell unit, it checks that `nacre-shell.service` is active with the Bead drop-in still loaded, then checks daemon health. The installer reports these checks explicitly. If the active shell is QML, the existing live QML validation and digest check remain unchanged.

`--dry-run --code-only` reports which shell it will validate. An unreadable or mixed shell status fails closed before deployment. The existing supervisor continues to treat `source.good` as the last validated QML tree and restores it on a failed QML startup. A later `nacre ui-shell qml` removes Bead's drop-in and restarts `nacre-shell`; its existing QML health check and supervisor startup validation apply then.

Tests use fake shell status, systemd, and daemon health. No live desktop or unit changes are part of this unit. No temporary duplicate implementation is introduced.

Acceptance evidence: focused installer fakes cover Bead validation without QML IPC, QML validation unchanged, and dry-run messaging; existing supervisor recovery and ui-shell QML recovery tests cover the later QML path. Full `desktop` CI is required before completion.
