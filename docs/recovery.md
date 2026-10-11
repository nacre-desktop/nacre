# Updates and recovery

Install system packages through pacman and keep Qt/Quickshell versions compatible;
avoid partial Qt updates. Source updates use committed, clean code and pinned
Orient dependencies. Run the plan before applying a managed deployment.

Managed releases snapshot owned code/configuration, validate source/IPC and real
launcher/Escape behavior, then promote. A failed gate restores the snapshot. Later
local edits are refused rather than overwritten. System packages, credentials,
wallets, browser profiles, conversations, memories and personal preferences are
outside code rollback.

`nacre doctor`, `nacre check`, `nacre profile`, `nacre session` and `nacre rollback`
expose health/recovery information. The source supervisor retains current/previous
known-good source. Brain keeps a source backup and preserves the browser profile.
A renderer update preserves a busy sidebar worker; worker code changes activate
when it is idle. Save pending drafts before explicitly restarting/restoring.

After updates, check the shell service and `hyprctl configerrors`. Actual cold
login, lock/suspend/resume and physical display/device acceptance need testing on
real hardware; a manual health check cannot establish those paths. General
installation guidance is in [installation](installation.md).
