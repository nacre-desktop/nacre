# N0409 — Updater follows AUR review policy

Status: implemented. The updater reads `nacre-helper tool aur-review policy` when using paru: `native` preserves `paru -Syu --skipreview`, while `required` or an unreadable policy runs `paru -Syu`; yay and pacman are unchanged. It prints the selected mode. Every profile defaults to required review; `native` only through the explicit `aurReview` setting (N0411 removed the legacy custom-profile branch). Focused shell tests cover both modes and policy failures.

No temporary duplicate or harness remains. Product line delta: +7, 2 removed (updates.sh).
