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

- Restore authorized GitHub publishing credentials, push this branch, create one
  follow-up PR and run required CI. Local code is complete; remote CI not run.
- Session agent owns backed-up in-place template integration, canonical palette
  include and palette publication; no shell restart by this task.

## Open questions / boundaries

- Publication blocked: GitHub CLI/GraphQL 401, HTTPS missing Git credentials,
  SSH public-key denied. Session agent also reports connector writes return 403.
  No authentication/profile or pre-push guard changes by this task.
- No running shell restart, staged-tool overwrite, session/profile edits or changes
  to Hyprland generated output. This is the requested appearance-parity correction.
- Runtime deployment and visual acceptance remain with the session agent (PR18).
