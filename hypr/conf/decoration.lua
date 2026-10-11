-- Nacre appearance baseline, captured from the maintained desktop.
-- The generated palette and private desktop choices load later.
hl.config({
    decoration = {
        rounding = 10,
        active_opacity = 1.0,
        inactive_opacity = 1.0,
        fullscreen_opacity = 1.0,
        blur = {
            enabled = true,
            size = 3,
            passes = 2,
            new_optimizations = true,
            ignore_opacity = true,
            xray = false,
        },
        shadow = {
            enabled = true,
            range = 20,
            render_power = 3,
            color = "rgba(00000040)",
        },
    },
})
