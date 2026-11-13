-- SPDX-License-Identifier: GPL-3.0-only
-- Notification toasts (design review D10): the title leads the header with "app · time" as its caption on the
-- right, the body in the variant ink, actions as chips with the first one filled. The app tile, caption and type
-- are the control panel's notification rows (N0238), so a toast and its history row read the same.
local ui=require("bead.ui")
local kit=require("bead.kit")
local measure=require("bead.widgets.measure")
local T=kit.tokens
local SANS="IBM Plex Sans"
local W,TILE,BUTTON,CHIP_H=380,28,28,30
local PAD_L,PAD_R,PAD_Y,GAP=T.spacing.lg,T.spacing.md,T.spacing.md,T.spacing.sm
local INNER=W-PAD_L-PAD_R
local TITLE,CAPTION=T.text.body*4/3,T.text.caption*4/3
-- `ui.text_width` measures the regular face; the 500 and 600 faces run a little wider.
local BOLD=1.06
local function textHeight(text,size,maximum,width)
    if text=="" then return 0 end
    local lines=0
    local columns=math.max(1,math.floor(width/(size*.55)))
    for line in (text.."\n"):gmatch("(.-)\n") do lines+=math.max(1,math.ceil((utf8.len(line) or #line)/columns)) end
    return math.min(maximum,lines)*size*1.4
end
local function chipWidth(label) return math.min(INNER,math.ceil(ui.text_width(label,SANS,CAPTION)*BOLD)+2*T.spacing.lg) end
local function actionRows(n)
    local rows={};local row={};local width=0
    for i,a in n.actions do
        -- The private owner marks its default as "id|default"; the daemon rows already omit it.
        if a.id==n.id.."|default" then continue end
        local w=chipWidth(a.label)
        if #row>0 and width+GAP+w>INNER then table.insert(rows,row);row={};width=0 end
        table.insert(row,{index=i,action=a,width=w});width+=w+(#row>1 and GAP or 0)
    end
    if #row>0 then table.insert(rows,row) end
    return rows
end
local function footerHeight(n)
    if not n.expanded then return 0 end
    local rows=#actionRows(n)
    return rows>0 and rows*CHIP_H+(rows-1)*GAP or 0
end
-- "app · time", at most 40% of the header so a long app name cannot squeeze the title out.
local function caption(n)
    return measure.ellipsize(n.app.." · "..(n.time_str or ui.tr("bead.notifications.time-now")),INNER*.4,SANS,CAPTION)
end
local function captionWidth(n) return math.ceil(ui.text_width(caption(n),SANS,CAPTION))+T.spacing.hairline end
-- The title shares the header line with the tile, the caption and the expand and dismiss buttons; a title longer
-- than its two collapsed lines also earns the expand button.
local function titleWidth(n,count) return INNER-TILE-3*T.spacing.compact-captionWidth(n)-count*BUTTON end
local function longTitle(n,width) return textHeight(n.summary,TITLE,3,width)>textHeight(n.summary,TITLE,2,width) end
local function hasDetails(n) return n.body~="" or #actionRows(n)>0 or longTitle(n,titleWidth(n,2)) end
local function buttons(n) return hasDetails(n) and 2 or 1 end
local function titleHeight(n) return textHeight(n.summary,TITLE,n.expanded and 6 or 2,titleWidth(n,buttons(n))) end
-- A collapsed title that needs more than its two lines ends in an ellipsis (with room for the wrap's ragged edge).
local function title(n)
    local w=titleWidth(n,buttons(n))
    if n.expanded or not longTitle(n,w) then return n.summary end
    return measure.ellipsize(n.summary,2*w*.85/BOLD,SANS,TITLE)
end
local function bodyHeight(n) return textHeight(n.body,CAPTION,n.expanded and 12 or 2,INNER) end
local function noticeHeight(n)
    local body,footer=bodyHeight(n),footerHeight(n)
    return 2*PAD_Y+math.max(TILE,titleHeight(n))+(body>0 and GAP+body or 0)+(footer>0 and GAP+footer or 0)
end
local function appIcon(n)
    local px=T.icons.medium*4/3
    local image
    if n.icon_path and n.icon_path~="" then image=ui.Image{id="app-icon",icon=n.icon_path,width=px,height=px,aria_label=n.app}
    else image=ui.Image{id="app-icon",path="notifications/fallback.themed.svg",width=px,height=px,aria_label=n.app} end
    return ui.Box{id="app-tile",width=TILE,height=TILE,radius=T.radii.small,fill="surfaceContainerHighest",shrink=0,padding=(TILE-px)/2,image}
end
local function iconButton(id,glyph,label,action)
    return kit.IconButton{id=id,text=glyph,aria_label=label,width=BUTTON,height=BUTTON,padding=0,radius=T.radii.full,shrink=0,
        font_size=T.icons.medium*4/3,fill="transparent",hover_fill="surfaceContainerHighest",color="onSurfaceVariant",on_activate=action}
end
-- An action chip; the first action is the primary one and is filled.
local function chip(item,primary)
    local label=item.action.label
    return kit.Button{id="action-"..item.index,width=item.width,height=CHIP_H,padding=0,shrink=0,radius=T.radii.full,
        fill=primary and "primaryContainer" or "surfaceContainerHigh",hover_fill=primary and ui.tint("primaryContainer","onPrimaryContainer",.08) or "surfaceContainerHighest",
        aria_label=label,on_activate=ui.action("notifications.action",item.action.ref or item.action.id),
        ui.Row{id="chip",width=item.width,height=CHIP_H,align="center",justify="center",
            ui.Text{id="label",text=label,width=item.width,height=18,text_align="center",family=SANS,font_size=CAPTION,font_weight=500,
                color=primary and "onPrimaryContainer" or "onSurface"}}}
end
local function notice(n)
    local key=n.key or n.id
    local controls={id="controls",width=buttons(n)*BUTTON,height=BUTTON,gap=0,shrink=0}
    if hasDetails(n) then
        table.insert(controls,iconButton("expand",n.expanded and "expand_less" or "expand_more",
            n.expanded and ui.tr("bead.notifications.collapse") or ui.tr("bead.notifications.expand"),ui.action("notifications.details",key)))
    end
    table.insert(controls,iconButton("dismiss","close",ui.tr("bead.notifications.dismiss"),ui.action("notifications.dismiss",key)))
    local footer={id="actions",gap=GAP,height=footerHeight(n),visible=n.expanded and #actionRows(n)>0}
    for i,items in actionRows(n) do
        local row={id="actions-row-"..i,gap=GAP,height=CHIP_H}
        for j,item in items do row[j]=chip(item,i==1 and j==1) end
        footer[i]=ui.Row(row)
    end
    return kit.Card{id="notice",height=noticeHeight(n),radius=T.radii.medium+T.spacing.hairline,padding=0,padding_left=PAD_L,padding_right=PAD_R,
        padding_top=PAD_Y,padding_bottom=PAD_Y,gap=GAP,fill="surfaceContainer",on_activate=ui.action("notifications.card",key),aria_label=n.app..": "..n.summary,
        border_width=1,border_color=n.urgency==2 and "error" or ui.alpha("outlineVariant",.45),
        ui.Row{id="header",width=INNER,height=math.max(TILE,titleHeight(n)),gap=T.spacing.compact,align="center",
            appIcon(n),
            ui.Text{id="summary",family=SANS,text=title(n),width=titleWidth(n,buttons(n)),height=titleHeight(n),font_size=TITLE,font_weight=600,shrink=0},
            ui.Text{id="caption",family=SANS,text=caption(n),width=captionWidth(n),height=18,shrink=0,text_align="right",font_size=CAPTION,color="onSurfaceVariant"},
            ui.Row(controls)},
        ui.Text{id="body",family=SANS,text=n.body,font_size=CAPTION,color="onSurfaceVariant",width=INNER,height=bodyHeight(n),visible=n.body~=""},
        ui.Column(footer),
    }
end
local rows=ui.resolve_for_check(ui.bind("notifications.rows"))
local height=math.max(0,#rows-1)*GAP
for _,n in rows do height+=noticeHeight(n) end
-- Motion (design review "Timings for 120 Hz", toast): 220 ms in, sliding 24 px from the right with the fade; 200 ms
-- out. D16: the kit opens with its EaseOut (the review asks out-quint) and closes with its accelerate curve, at most
-- .75 x the open duration (165 ms), until the B0263 easing tokens land.
return ui.Column{edge="right",id="notifications",width=W,height=math.max(1,math.min(720,height)),fill="transparent",radius=0,
    open=ui.bind("notifications.open"),duration=.22,return_duration=.2,slide_distance=24,
    require("bead.widgets.kit.virtual-list"){id="stream",width=W,height=math.max(1,math.min(720,height)),
        scroll=ui.bind("notifications.scroll.y"),model=ui.bind("notifications.rows"),key="id",
        row_height=noticeHeight,gap=GAP,render=notice},
}
