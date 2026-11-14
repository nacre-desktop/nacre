-- SPDX-License-Identifier: GPL-3.0-only
-- One notification row (design review D4/D9): the app icon in a tinted square, "app · time", the
-- 600-weight summary, one body line and a dismiss button. Shared by the control panel and the
-- dashboard's Notifications page. `n` is {id, app, when, summary, body, icon_path};
-- `p` is {width, fill?, on_dismiss?} (nil disables the button).
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local M={height=72,tile=28}
local function icon(n)
    local px=T.icons.medium*4/3
    local image
    if n.icon_path and n.icon_path~="" then image=ui.Image{id="app-icon",icon=n.icon_path,width=px,height=px,aria_label=n.app}
    else image=ui.Image{id="app-icon",path="notifications/fallback.themed.svg",width=px,height=px,aria_label=n.app} end
    return ui.Box{id="app-tile",width=M.tile,height=M.tile,radius=T.radii.small,fill="surfaceContainerHighest",shrink=0,padding=(M.tile-px)/2,image}
end
function M.Row(n,p)
    local BODY,CAPTION=T.text.body*4/3,T.text.caption*4/3
    local caption=n.when and n.when~="" and n.app.." · "..n.when or n.app
    local text_w=p.width-T.spacing.md*2-M.tile-T.spacing.compact*2-M.tile
    return ui.Row{id="notice-"..n.id,width=p.width,height=M.height,shrink=0,radius=T.radii.medium,fill=p.fill or "surfaceContainer",
        padding_left=T.spacing.md,padding_right=T.spacing.md,gap=T.spacing.compact,align="center",
        icon(n),
        ui.Column{id="notice-text",width=text_w,gap=0,
            ui.Text{id="app",text=caption,width=text_w,height=16,font_size=CAPTION,color="onSurfaceVariant"},
            ui.Text{id="summary",text=n.summary,width=text_w,height=20,font_size=BODY,font_weight=600,color="onSurface"},
            ui.Text{id="body",text=n.body,width=text_w,height=16,font_size=CAPTION,color="onSurfaceVariant"}},
        kit.IconButton{id="dismiss",text="close",aria_label=ui.tr("bead.notifications.dismiss"),width=M.tile,height=M.tile,padding=0,radius=T.radii.full,shrink=0,
            font_size=T.icons.medium*4/3,fill="transparent",hover_fill="surfaceContainerHighest",color="onSurfaceVariant",
            on_activate=p.on_dismiss}}
end
return M
