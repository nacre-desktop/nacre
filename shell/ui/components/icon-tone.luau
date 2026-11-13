-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
return function()
    return ui.tint("onSurfaceVariant","primary",ui.resolve_for_check(ui.bind("launcher.icon_tint")))
end
