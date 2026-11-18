-- SPDX-License-Identifier: GPL-3.0-only
-- QML NacreHeader / NacreWorkspaceRow geometry, in logical pixels.
return {
    brand_size=32,
    line_height=1.4, text_lift=2/3, icon_font=20, icon_width=24, detail_font=40/3,
    clock_font=15, clock_min_width=44, status_height=34, status_radius=17, item_gap=3,
    -- D1 (spec 0237): 20 px glyphs in 24 px boxes, 14 px between boxes, 12 px to the pill ends.
    -- The 7 px padding is each item's half of the gap, so the buttons tile with no seam.
    status_gap=12, power_gap=9, status_padding=7, status_count_padding=7, status_hit_pad=0, status_end_padding=5,
    -- Owner 2026-11-18 (live, overrides the locked calm ink): back to the 2026-11-14 scheme, one role per group:
    -- icons secondary, the clock tertiary, the power button error; badges stay red (error). Hover is an onSurface wash.
    status_tone="secondary", charge_tone="primary", clock_tone="tertiary", power_tone="error", power_hover_tone="error", status_hover=.1,
    badge_size=15, badge_font=10, badge_pad=4, badge_lift=5, badge_overhang=7, separator_height=16,
    status_text_offset=0, clock_text_offset=2/3,
    pill_height=34, pill_radius=17, pill_gap=0, power_size=34,
    workspace_height=30, workspace_radius=15, workspace_number_font=11,
    active_height=28, active_radius=14, app_font=14, hint_icon=16,
    -- D2 stack: a named 28 px cap (Settings > Panels > Top bar > Stack style); the default strand is the stack pearls below.
    strand_overlap=4, strand_pad=3,
    cap_height=28, cap_radius=14, cap_font=12, cap_icon=14, cap_gap=5, cap_pad_left=6, cap_pad_right=8, cap_chars=12,
    -- Calm left island (locked 2026-11-17): one pearl size, larger numbers, app icons only in the active pearl and
    -- only on outputs at least `hints_min_width` logical px wide.
    pearl_size=22, pearl_ring_size=21, pearl_number_font=13, hints_min_width=1600,
    -- The Colors page still draws the older pearls from these.
    pearl_current=22, pearl_occupied=20, pearl_empty=18, pearl_empty_border=1.5,
    -- Stack pearls: the current one 10 px inside a white 1.5 px ring, a 3 px accent ring and a glow; the rest 7.5 px
    -- at 70 %; a fanned strand shows numbers in 16 px pearls (at most 8), a collapsed one three around the current.
    stack_current=10, stack_other=7.5, stack_ring=1.5, stack_accent_ring=3, stack_glow=3, stack_other_alpha=.7, stack_gap=4, stack_fan_size=16, stack_collapsed_max=3, stack_fan_max=8,
    pearl_cream={255,249,241,255}, pearl_white={255,255,255,255}, pearl_dark={48,48,54,255}, pearl_light={245,242,238,255},
    title_height=30, title_font=15, title_ink="primary", title_icon=16, title_max_width=550, title_cue_max=560, title_gap=8, title_trace_height=2, title_trace_inset=5, title_trace_width=36,
}
