-- Native desktop tools. Existing app, workspace and power shortcuts stay in place.
hl.bind("SUPER + SPACE", hl.dsp.exec_cmd("~/.local/bin/nacre-shell palette"))
hl.bind("SUPER + K", hl.dsp.exec_cmd("~/.local/bin/nacre-menu learn.shortcuts"))
hl.bind("SUPER + CTRL + B", hl.dsp.exec_cmd("~/.local/bin/nacre-shell left"), { description = "Nacre:ai-sidebar" })

-- Passive hover surfaces do not steal keyboard focus. Escape still reaches apps.
hl.bind("Escape", hl.dsp.global("nacre_shell:dismissHoverEdges"), { non_consuming = true })
