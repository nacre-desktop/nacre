# Quiet SSH startup

An unconditional Fastfetch invocation in Fish's startup fragments writes a logo
to noninteractive SSH stdout. This can corrupt SCP transfers and remote tool
protocols. `python3 ai/fix-fish-presentation.py` recognizes the specific
render-logo/Fastfetch block, preserves a dated original, checks Fish syntax, and
wraps only presentation output in `status is-interactive`. Other shell setup and
PATH handling remain active. Unexpected startup content fails without edits.

Verify `ssh HOST true` produces zero stdout bytes, then rerun a small file
transfer. Restore the exact dated startup-file backup to roll back the change.
