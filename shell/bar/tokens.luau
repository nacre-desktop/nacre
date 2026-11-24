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
    -- icons secondary, the clock primary (tertiary is near the ink on a quarter of the palettes), the power button error; glyphs inside the icons (badge pill and numbers, bolt, values) share one wallpaper role, glyph_tone. Hover is an onSurface wash.
    status_tone="secondary", glyph_tone="tertiary", clock_tone="primary", power_tone="error", power_hover_tone="error", status_hover=.1,
    badge_size=15, badge_font=10, badge_pad=4, badge_lift=5, badge_overhang=7, separator_height=16,
    status_text_offset=0, clock_text_offset=2/3,
    pill_height=34, pill_radius=17, pill_gap=0, power_size=34,
    workspace_height=30, workspace_radius=15, workspace_number_font=11,
    active_height=28, active_radius=14, app_font=14, hint_icon=16,
    -- D2 stack: a named 28 px cap (Settings > Panels > Top bar > Stack style); the default is the stack number below.
    strand_overlap=4, strand_pad=3,
    cap_height=28, cap_radius=14, cap_font=12, cap_icon=14, cap_gap=5, cap_pad_left=6, cap_pad_right=8, cap_chars=12,
    -- Calm left island (locked 2026-11-17): one pearl size, larger numbers, app icons in every occupied pearl.
    -- Workspace pearls (QML NacreWorkspacePearl), themed SVG gloss from the wallpaper palette.
    pearl_current=22, pearl_occupied=20, pearl_empty=18, pearl_current_border=1, pearl_empty_border=1.5,
    -- Stack number (spec 0253): the current stack's numeral in the 28 px cap, rolled in 180 ms on a switch; fanned,
    -- one 20 px numeral slot per stack (at most 8, then "+N").
    stack_font=15, stack_slot=20, stack_roll=.18, stack_gap=4, stack_fan_max=8,
    pearl_cream={255,249,241,255}, pearl_dark={48,48,54,255}, pearl_light={245,242,238,255},
    title_height=30, title_font=15, title_ink="primary", title_icon=16, title_max_width=550, title_cue_max=560, title_gap=8, title_trace_height=2, title_trace_inset=5, title_trace_width=36,
}
