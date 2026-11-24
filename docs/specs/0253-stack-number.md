# 0253 Stack number: one mark that shows the stack number

## Problem

Owner, 2026-11-23 22:55, looking at the live bar (laptop eDP-1, scale 1.5, one stack): "i dont
really like the square for the stack indicator either, maybe a single thing that just shows the
stack number like a 1 a 2 or a 3 etc". Today (5ea9aaca8, 728ba90f0) the stack control is a 28 px
`surfaceContainerHighest` capsule at the head of the workspaces pill, then a hairline divider. It
holds one rounded square per stack: the current one 13 px in a white ring, an accent ring and a
glow, the others 8 px at 70 %, 4+ stacks folded to three and "+N". The square carries no number,
so the owner can't read which stack is current.

## Rules every concept keeps

- One mark at rest: the number of this output's current stack (`index` of the row in
  `bar.collections`, the number the switcher shows as "Stack N"; list position if it has none).
- No hover element over the mark (0883f7723: the in-bar name tooltip was a pill-height dark circle
  that covered the square for the whole hover; the bar surface is only bar-high, so any tooltip
  drawn there covers it). Hover is the capsule's existing `onSurface` 3 % wash. The name stays
  one click away in the switcher and in the control's aria label. The 400 ms hover fan stays.
- Click unchanged: 2+ stacks fan the strand (each other stack a switch button, "+N" past 8 opens
  the switcher); one stack opens the switcher (the way to add a second).
- Switching rolls the numeral like an odometer: one `ui.animate` (180 ms, out-cubic) on the
  current stack's position drives two absolutely placed numerals in a fixed, clipped box, so a
  frame moves two text offsets and nothing else lays out. Up when the number grows, down when it
  shrinks. It runs only while stacks show, so stacks that arrive after the first frame (startup,
  the drag ghost) snap to their number instead of rolling up from 1. Reduced motion snaps.
- Tier 2 (stock niri, Hyprland): `bar.collections` is empty, the control and divider stay hidden.
- `bar.stack.style = "name"` (the named cap) is untouched.
- Geometry in logical px from `shell/bar/tokens.luau`, colours from palette roles; Bead scales,
  so laptop 1.5 and external 1.0 share one layout (renders at both).

## Concepts

Renders (unit worktree, not committed): `.lead/tmp/concepts/<A|B|C>-<dark|light>-<1.5|1>-<1|3>stacks.png`
(frame bar, 1920 px, real renderer), `<X>-dark-1.5-roll-mid.png` (60 ms into a 1 -> 2 roll),
`NOW-*` (today's squares), side by side in `sheet.png`.

### A: the numeral (type only)

- The 28 px round capsule keeps its fill; inside, only the number, 15 px / 600 `primary`. No
  shape behind it, so it can't read as a workspace pearl (pearls are enamel discs with a dark
  number; this is accent ink on the capsule).
- 1, 3 or 6 stacks look the same at rest: one numeral. No count hint (the fan shows the rest).
- Fanned: the numerals in a row in the same capsule, the current one `primary` 600, the others
  `onSurfaceVariant` 500, each a 20 px button.
- Light: `primary` on the light `surfaceContainerHighest`; dark: the same roles. Renders: the
  numeral reads as a label (accent ink, no disc), clearly apart from the enamel pearls, in both.

### B: the enamel lozenge

- A horizontal 22x18 px lozenge (radius 9) in the lit-pearl enamel (`primary` warmed toward
  `pearl_cream`, as a flat-fill pearl) with the number 11 px / 600 in `pearl_dark`, inside the
  capsule. Wider than tall, so neither a pearl (disc) nor the old square.
- Count hint: with 2+ stacks, "/N" in 10 px `onSurfaceVariant` after the lozenge ("2/3", "2/6");
  one stack shows the lozenge alone.
- Fanned: one lozenge per stack, the current enamel, the others at 55 %.
- Renders: in dark the cream lozenge with a dark "2" reads as one more workspace pearl beside the
  "1" pearl; in light the flat enamel is a muted green blob. "/3" is legible but small at 1.0.

### C: the card stack

- The number (11 px / 600 `pearl_dark`) on an 18x15 px enamel card (radius 4), with one card
  edge peeking above per extra stack (at most two, 55 % and 30 %), the classic "stack" glyph.
- The peeks are the count hint (1: a plain card, 2: one edge, 3+: two edges).
- Fanned: one card per stack, the current with its edges.
- Renders: at 28 px the two peeks merge into a bell shape over the card, and one stack is still
  a rounded square, now with a number.

## The pick: A, the numeral

- It is literally the owner's "single thing that just shows the stack number": one glyph, no shape.
- It is the only concept that can't be mistaken for a workspace pearl: B's enamel lozenge and C's
  enamel card both borrow the pearl's cream-on-dark language right next to the pearls.
- No count hint at rest: the owner asked for the number only; the fan (click or hover) shows the
  other stacks, and B's "/N" was the noisiest part of B at scale 1.0.
- Cheapest: one Text (two during the 180 ms roll), no fills; the smallest diff and the most deleted.
- Both palettes come from the `primary` and `surfaceContainerHighest` roles, which already pass in
  the bar; scale 1.5 and 1.0 render the same layout (Bead scales logical px).

## Owner check

At rest the stack capsule left of the hairline holds one number in the accent colour: "1" with one
stack, and with two or more the number of this screen's current stack (each screen shows its own).
Switching stacks (Super+Ctrl+Tab, the switcher or a fanned number) rolls the old number out and the
new one in, upward when the number grows, in 180 ms; nothing else moves. At login the number
appears as soon as the stacks are known, already right, without a roll. Hovering shows only the
capsule's faint wash, never a tooltip over the number; with 2+ stacks, staying 400 ms fans out every
stack's number in the same capsule (current in the accent, the others muted). Clicking with one stack
opens the switcher; with 2+ it fans at once, and clicking another number switches to that stack.

## Implementation (A)

- `shell/bar/tokens.luau`: `stack_current`, `stack_other`, `stack_ring`, `stack_accent_ring`,
  `stack_glow`, `stack_other_alpha`, `stack_corner`, `stack_fan_size`, `stack_collapsed_max` and
  `pearl_white` (the square's white ring) go; `stack_font=15`, `stack_slot=20` (one fanned
  numeral), `stack_roll=.18` come; `stack_gap`, `stack_fan_max` stay.
- `shell/widgets/workspaces/widget.luau`: at rest the 28 px capsule holds the rolling numeral in a
  clipped box (`stack-number`); fanned, one 20 px numeral button per stack (`stack-strand-k`,
  current `primary` 600, others `onSurfaceVariant` 500), "+N" past 8. Deleted: the square layers
  (glow, accent and white rings, back card), the per-stack sizes and the at-rest three-and-"+N"
  fold. `collection-badge`, `stack-strand-*` and `stack-more` keep their ids, so hover, click and
  the fan keep their hooks. The name cap and the Tier 2 gate are unchanged.
- Tests (`service_views_tests.rs`, helper `stack_numbers`): at rest the mark is the current
  stack's number alone (5 stacks, current 3: "3"; the desk-chip cases "1", "2", "1"); each output's
  bar shows its own current stack's index, and follows a swap; fanned, 5 stacks show "1".."5" and 3
  stacks "1" "2" "3" with two switch buttons. The click pin and no-tooltip checks stay.
- Goldens: the stack-control ones (`bar-stacks-*`, `bar-narrow-*`, `bar-screens-*`,
  `desk-chip-*`) refreshed; new `bar-stacks-fan3-1.5` and `bar-stacks-fan3-light-1.5` (3 stacks
  fanned on the frame bar). `bar-states-*` (the name cap) and every other golden are unchanged.
  No new strings.
