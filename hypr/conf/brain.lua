-- Nacre Brain routes; legacy classes remain compatible with open browser windows.
hl.window_rule({
    name = "nacre-brain",
    match = { class = "^(nacre-brain|nacre-brain|chrome-.*(nacre_brain|nacre-observatory)_auth_open[.]html-.*)$" },
    fullscreen_state = "0 0",
    suppress_event = "fullscreen maximize",
})
hl.window_rule({
    name = "nacre-brain-chrome",
    match = { class = "^chrome-127[.]0[.]0[.]1__-Default$" },
    fullscreen_state = "0 0",
    suppress_event = "fullscreen maximize",
})
hl.window_rule({ name = "nacre-obsidian", match = { class = "^([Oo]bsidian)$" } })
hl.bind("SUPER + B", hl.dsp.exec_cmd("~/.local/bin/nacre-brain open"))
hl.bind("SUPER + N", hl.dsp.exec_cmd("~/.local/bin/nacre-brain capture"))

-- A credential handoff updates an existing browser profile, then closes itself.
hl.window_rule({
    name = "brain-auth-handoff",
    match = { class = "^(nacre-brain-auth|nacre-brain-auth|chrome-.*(nacre_brain|nacre-observatory)_auth_handoff[.]html-.*)$" },
    float = true,
    no_initial_focus = true,
    size = "80 80",
})
