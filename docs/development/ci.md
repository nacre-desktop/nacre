# Development checks

The desktop job in [check.yml](../../.github/workflows/check.yml) is the complete
source gate. It runs Rust formatting, Clippy and tests; Bead product, Luau and proof checks;
translation checks; source and UI regressions; and native gallery checks. Use its Arch container environment for the full gate.

From the repository root, the source and native UI regression step is:

```sh
dbus-run-session -- xvfb-run -a /tmp/check-env/bin/python tools/check.py
```

The README links to the [product-shell build and preview guide](product-shell-build.md).
