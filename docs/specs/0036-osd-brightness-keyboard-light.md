# N0036 — Brightness OSD value and keyboard-light method

Status: **Implemented; pending lead review**. Consumer: Bead B0026 blockers E
and B (`nacrectl ui keyboardLight step up|down`, brightness OSD feedback).
Additive to N0031 (`brightness.*` stays v1; `keyboard_light.adjust` is new v1).

## Contract

- `brightness.set` / `brightness.adjust` reply
  `{changed:true, output, level_percent}`. `output` echoes the requested output
  name; `level_percent` (integer 0..100) is the device value read back after the
  write (backlight sysfs or DDC re-read), rounded. The QML display OSD shows a
  0..1 level, a low/high glyph (below/above 50%) and the output name only when
  more than one screen exists; the shell derives all three from these fields and
  its own output count. New fields are additive; old consumers that only read
  `changed` are unaffected.
- `keyboard_light.adjust {step:"up"|"down"}` replies
  `{changed, level, max}` (raw integers, `max >= 1`). Semantics mirror
  `NacreKeyboardLight.step`: one raw unit; `up` wraps from `max` to 0, `down`
  stops at 0. `changed:false` (down at 0) issues no write. The write reuses the
  identity-checked `light_devices::write` (`brightnessctl`, `kbd_backlight` LED
  only); callers never supply a device.
- Absent keyboard backlight: the method is not in the welcome registry (needs
  `brightnessctl` and a `*kbd_backlight*` LED) and a direct call fails
  `unavailable`. Safe mode refuses. It shares the brightness lane, so a
  concurrent brightness or keyboard action fails `busy` (no queued relative
  actions). Capability class `system`, default deadline 30 s.

## Proposal (not implemented): single `osd` topic

The QML OSD is fed from per-source signals (`NacreAudio`, monitor `adjusted`,
`NacreKeyboardLight.adjusted`, caps lock, power profile, night light, DND), not
from one place, and it mixes daemon-owned and shell-local state. A published
`osd` topic would have to re-publish those sources and invent ordering/dedup,
so reply values are cheaper and sufficient for key-driven feedback. Revisit
only if a shell needs OSD for changes it did not request (other clients).

## Acceptance

Regenerated schemas/Luau/goldens (`export` example); new golden call
`keyboard_light.adjust`; protocol tests (14 typed goldens); helper tests with
fake sysfs and a fake `brightnessctl` for percent readback, up-wrap, down-stop,
no-write-when-unchanged and absent device; safe-mode refusal. QML unchanged.
Whole desktop CI job in an Arch container on dev-2. No live/hardware use. No
temporary duplicates added.
