# Niri gradient palette follow-up

## Done

- Isolated `fix/niri-gradient-palette` branch from main `b547d06`.
- Restored active primary-to-secondary gradient at 45 degrees in both focus ring
  and border, preserving solid fallback, inactive/urgent colors, widths and flags.
- Required Orient roles: `primary`, `secondary`, `outlineVariant`, `error`.
- Test compares both niri gradients with the existing Hyprland palette role order
  and angle; native KDL fixture substitutes distinct colors for all four roles.
- Session agent owns the canonical palette include path and runtime acceptance.

## Next

- Targeted config tests, native niri validation and full `tools/check.py`.
- Publish one follow-up PR and return exact tested revision to the session agent.

## Open questions / boundaries

- GitHub GraphQL currently returns 401; publication uses existing credentials only.
- No running shell restart, staged-tool overwrite, session/profile edits or changes
  to Hyprland generated output. This is the requested appearance-parity correction.
- Runtime deployment and visual acceptance remain with the session agent (PR18).
