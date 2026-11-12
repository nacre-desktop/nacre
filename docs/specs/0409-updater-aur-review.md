# N0409 — Updater follows AUR review policy

Status: implemented. The updater reads `nacre-helper tool aur-review policy` when using paru: `native` preserves `paru -Syu --skipreview`, while `required` or an unreadable policy runs `paru -Syu`; yay and pacman are unchanged. It prints the selected mode. Public users default to required review; the owner's legacy custom profile remains native. Focused shell tests cover both modes and policy failures.

No temporary duplicate or harness remains. Product line delta: +7, 2 removed (updates.sh).
