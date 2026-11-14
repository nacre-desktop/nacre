-- SPDX-License-Identifier: GPL-3.0-only
-- Notifications (design review D9): one card, the title with an All | DND segment, the control
-- panel's notification rows (shared component) and a Clear chip; the empty state is the shared one.
local ui=require("bead.ui")
local kit=require("bead.kit")
local notice=require("../../ui/components/notice-row")
local empty=require("../../ui/components/empty-state")
local segment=require("../../widgets/segment")(ui,kit)
local VirtualList=require("bead.widgets.kit.virtual-list")
local T=kit.tokens
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local dnd=ui.resolve_for_check(ui.bind("dashboard.dnd"))
    local rows=ui.resolve_for_check(ui.bind("dashboard.rows"))
    local note=ui.resolve_for_check(ui.bind("dashboard.detail"))
    local W,H,PAD=940,562,T.spacing.md+T.spacing.xs
    local INNER=W-2*PAD
    local HEAD,CAPTION=36,T.text.caption*4/3
    local listTop=PAD+HEAD+T.spacing.md
    local listHeight=H-listTop-PAD
    local function act(op) return control and ui.action("dashboard.do",op) or nil end
    local body
    if #rows==0 then
        body=empty{top=listTop,width=INNER,height=listHeight,glyph="notifications",
            title=ui.tr("bead.dashboard.notifications-none"),detail=note}
    else
        body=VirtualList{id="items",position="absolute",left=PAD,top=listTop,scroll=ui.bind("dashboard.notifications.scroll"),
            width=INNER,height=listHeight,gap=T.spacing.sm,row_height=notice.height,model=ui.bind("dashboard.rows"),key="id",repeat_id="items",
            render=function(item) return notice.Row(item,{width=INNER,fill="surfaceContainerHigh",on_dismiss=act(item.act)}) end}
    end
    local clear=ui.tr("bead.dashboard.notifications-clear")
    -- title left, the actions (count note, All | DND, Clear) as one right-aligned group
    local actions={id="actions",height=HEAD,gap=T.spacing.md,align="center"}
    if #rows>0 and note~="" then
        actions[#actions+1]=ui.Text{id="note",text=note,width=math.min(300,math.ceil(ui.text_width(note,"IBM Plex Sans",CAPTION))+T.spacing.xs),height=18,
            shrink=0,font_size=CAPTION,text_align="right",color="onSurfaceVariant"}
    end
    actions[#actions+1]=segment.Segment{id="dnd",aria_label=ui.tr("bead.dashboard.page-notifications"),enabled=control,on_change="dashboard.do",
        options={{id="notif.all",label=ui.tr("bead.dashboard.notifications-all"),selected=not dnd},
            {id="notif.dnd",label=ui.tr("bead.panel.control-pill-dnd"),selected=dnd}}}
    if #rows>0 then
        actions[#actions+1]=kit.PillButton{id="clear",compact=true,glyph="clear_all",label=clear,
            label_width=ui.text_width(clear,"IBM Plex Sans",CAPTION),on_activate=act("notif.clear")}
    end
    local header=ui.Row{id="header",position="absolute",left=PAD,top=PAD,width=INNER,height=HEAD,justify="space-between",align="center",
        ui.Text{id="title",text=ui.bind("dashboard.heading"),width=INNER*.4,height=24,font_size=T.text.body*4/3,font_weight=600,color="onSurface"},
        ui.Row(actions)}
    return ui.Box{id="notifications",width=W,height=H,radius=T.spacing.xl,fill="surfaceContainer",header,body}
end
