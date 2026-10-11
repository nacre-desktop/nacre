-- Nacre overlays and the small external tools still used by desktop helpers.
-- Workspace placement is owned by windowrule.lua and Brain-specific routing.
hl.layer_rule({
    name = "nacre-overlay-glass",
    match = { namespace = "^nacre-(drawers|topbar|lock-preview|left-menu-handle|right-menu-handle)$" },
    blur = true,
    ignore_alpha = 0.5,
    no_anim = true,
})

hl.window_rule({
    name = "nacre-lock-preview",
    match = { class = "^org.quickshell$", title = "^Nacre lock screen preview$" },
    float = true,
    center = true,
    size = "70% 70%",
})
hl.window_rule({
    name = "nacre-audio-mixer",
    match = { class = "^org\\.pulseaudio\\.pavucontrol$" },
    float = true,
    center = true,
    pin = true,
    size = "700 600",
})
hl.window_rule({
    name = "nacre-bluetooth-manager",
    match = { class = "^blueman-manager$" },
    float = true,
    center = true,
    size = "800 600",
})
hl.window_rule({
    name = "nacre-network-editor",
    match = { class = "^nm-connection-editor$" },
    float = true,
    center = true,
    size = "800 700",
})
hl.window_rule({
    name = "nacre-share-picker",
    match = { class = "^hyprland-share-picker$" },
    float = true,
    center = true,
    pin = true,
    size = "600 400",
})
hl.window_rule({
    name = "nacre-picture-in-picture",
    match = { title = "^[Pp]icture[ -]?[Ii]n[ -]?[Pp]icture.*$" },
    float = true,
    center = true,
    pin = true,
})
hl.window_rule({
    name = "nacre-native-controls",
    match = { class = "^nacre-control$" },
    float = true,
    center = true,
    size = "720 500",
})

hl.config({ xwayland = { force_zero_scaling = true } })

-- Settings is a normal resizable window on the current workspace.
hl.window_rule({
    name = "nacre-settings-app",
    match = { class = "^org[.]quickshell$", title = "^Nacre Settings$" },
    float = true,
    center = true,
})

hl.window_rule({
    name = "nacre-colors-app",
    match = { class = "^org[.]quickshell$", title = "^Nacre Colors$" },
    float = true,
    center = true,
})

-- Welcome is a normal single-instance app, sized by its responsive Qt window.
hl.window_rule({
    name = "nacre-welcome",
    match = { class = "^org[.]quickshell$", title = "^Nacre Welcome$" },
    float = true,
    center = true,
})
