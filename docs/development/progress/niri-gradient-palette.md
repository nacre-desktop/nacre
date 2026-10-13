# Niri gradient palette follow-up

## Done

- Isolated `fix/niri-gradient-palette` branch from main `b547d06`.
- Restored active primary-to-secondary gradient at 45 degrees in both focus ring
  and border, preserving solid fallback, inactive/urgent colors, widths and flags.
- Required Orient roles: `primary`, `secondary`, `outlineVariant`, `error`.
- Test compares both niri gradients with the existing Hyprland palette role order
  and angle; native KDL fixture substitutes distinct colors for all four roles.
- Session agent owns the canonical palette include path and runtime acceptance.

- Validation completed on code commit `b2a8527`: 10 focused config tests and
  native niri 26.04 parsing passed; full `tools/check.py` passed all 709 tests,
  lint/format, source/native and generator checks. Hyprland generated files match.
- The child source-check environment selects Hyprland for binding validation and
  removes inherited Qt logging suppression/pattern settings; session env untouched.
- Exact tested template/revision and required roles returned to the session agent.

## Next

- One follow-up PR and required final-head CI; local code and validation are
  complete. Publishing credentials now verify and the task branch is pushed.
- Session agent owns backed-up in-place template integration, canonical palette
  include and palette publication; no shell restart by this task.

## Open questions / boundaries

- Earlier CLI/Git authentication failures were resolved by the operator; existing
  credentials now verify. No authentication/profile or guard edits by this task.
- Session agent reports consuming only code commit `b2a8527`'s tested template with
  private backups, pure palette publication, native validation/config reload and
  unchanged shell/compositor PIDs. No runtime/staged writes by this task.
- No running shell restart, staged-tool overwrite, session/profile edits or changes
  to Hyprland generated output. This is the requested appearance-parity correction.
- Runtime deployment and visual acceptance remain with the session agent (PR18).
