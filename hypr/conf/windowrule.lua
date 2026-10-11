-- Generic file-manager window behavior; app/workspace routing belongs to the selected profile.
-- Files stays an unpinned popup on the current workspace.
hl.window_rule({
    name = "nacre-thunar-files",
    match = { class = "^([Tt]hunar)$" },
    float = true,
    center = true,
    size = "70% 75%",
})

-- Dolphin opens wider for its Places sidebar and file grid, on the current workspace.
hl.window_rule({
    name = "nacre-dolphin-files",
    match = { class = "^(org[.]kde[.]dolphin|[Dd]olphin)$" },
    float = true,
    center = true,
    size = { "monitor_w*0.6", "monitor_h*0.75" },
})
