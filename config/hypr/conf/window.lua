-- Nacre placement defaults; palette and private desktop overrides load last.
hl.config({
    ["general.layout"] = "dwindle",
    ["general.gaps_in"] = 6,
    ["general.gaps_out"] = 12,
    ["general.float_gaps"] = -1,
    ["general.border_size"] = 3,
    ["general.resize_on_border"] = true,
    ["general.col.active_border"] = {
        colors = { var_primary, var_on_primary },
        angle = 90,
    },
    ["general.col.inactive_border"] = var_on_primary,
})
