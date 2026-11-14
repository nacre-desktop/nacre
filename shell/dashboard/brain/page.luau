-- SPDX-License-Identifier: GPL-3.0-only
-- Brain (design review D9): recent notes as rows that open the Brain app (`nacre brain <id>`),
-- open loops as checkbox rows, "Open Brain" as the card action; shared empty states.
local ui=require("bead.ui")
local kit=require("bead.kit")
local empty=require("../../ui/components/empty-state")
local T=kit.tokens
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local notes=ui.resolve_for_check(ui.bind("dashboard.rows"))
    local tasks=ui.resolve_for_check(ui.bind("dashboard.secondary"))
    local W,H,PAD,GUTTER=940,562,T.spacing.md+T.spacing.xs,T.spacing.lg
    local LEFT=math.floor((W-GUTTER)*7/12)
    local RIGHT=W-GUTTER-LEFT
    local HEAD,ROW=36,40
    local BODY,CAPTION=T.text.body*4/3,T.text.caption*4/3
    local open=control and ui.action("dashboard.do","brain.open") or nil
    local openLabel=ui.tr("bead.dashboard.brain-open")
    local function row(id,glyph,text,width,action)
        local inner=ui.Row{id="row",width=width-2*T.spacing.compact,height=24,gap=T.spacing.compact,align="center",
            ui.Text{id="glyph",text=glyph,family=T.fonts.icon,font_size=T.icons.large*4/3,width=24,height=24,text_align="center",color="onSurfaceVariant"},
            ui.Text{id="label",text=text,width=width-2*T.spacing.compact-24-T.spacing.compact,height=24,font_size=BODY,color="onSurface"}}
        return kit.Button{id=id,width=width,height=ROW,padding=T.spacing.compact,padding_top=(ROW-24)/2,padding_bottom=(ROW-24)/2,radius=T.radii.small,
            fill="transparent",hover_fill=ui.alpha("onSurface",.06),aria_label=text,on_activate=action,inner}
    end
    local function list(id,model,glyph,width)
        return ui.Column{id=id,width=width,gap=T.spacing.hairline,
            ui.Repeat{id="items",model=ui.bind(model),key="id",render=function(item)
                return row(id.."-"..item.id,glyph,item.label,width,item.act and control and ui.action("dashboard.do",item.act) or nil)
            end}}
    end
    local notesInner,tasksInner=LEFT-2*PAD,RIGHT-2*PAD
    local notesBody,tasksBody
    if #notes==0 then
        notesBody=empty{top=PAD+HEAD,width=notesInner,height=H-2*PAD-HEAD,glyph="psychology",title=ui.tr("bead.dashboard.brain-empty"),
            detail=ui.tr("bead.dashboard.brain-empty-detail"),chip=openLabel,chip_glyph="hub",on_chip=open}
    else
        notesBody=ui.Column{id="notes-body",position="absolute",left=PAD,top=PAD+HEAD+T.spacing.md,width=notesInner,gap=T.spacing.sm,
            ui.Text{id="caption",text=ui.bind("dashboard.heading"),width=notesInner,height=18,font_size=CAPTION,color="onSurfaceVariant"},
            list("notes","dashboard.rows","description",notesInner)}
    end
    if #tasks==0 then
        tasksBody=empty{top=PAD+HEAD,width=tasksInner,height=H-2*PAD-HEAD,glyph="task_alt",title=ui.tr("bead.dashboard.brain-no-tasks")}
    else
        tasksBody=ui.Column{id="tasks-body",position="absolute",left=PAD,top=PAD+HEAD+T.spacing.md,width=tasksInner,gap=T.spacing.sm,
            list("tasks","dashboard.secondary","check_box_outline_blank",tasksInner)}
    end
    return ui.Box{id="brain",width=W,height=H,
        ui.Box{id="notes",position="absolute",left=0,top=0,width=LEFT,height=H,radius=T.spacing.xl,fill="surfaceContainer",
            ui.Row{id="header",position="absolute",left=PAD,top=PAD,width=notesInner,height=HEAD,justify="space-between",align="center",
                ui.Text{id="title",text=ui.tr("bead.dashboard.page-brain"),width=notesInner-200,height=24,font_size=BODY,font_weight=600,color="onSurface"},
                kit.PillButton{id="open",compact=true,glyph="hub",label=openLabel,label_width=ui.text_width(openLabel,"IBM Plex Sans",CAPTION),on_activate=open}},
            notesBody},
        ui.Box{id="tasks",position="absolute",left=LEFT+GUTTER,top=0,width=RIGHT,height=H,radius=T.spacing.xl,fill="surfaceContainer",
            ui.Text{id="title",position="absolute",left=PAD,top=PAD+(HEAD-24)/2,width=tasksInner,height=24,text=ui.tr("bead.dashboard.brain-tasks"),
                font_size=BODY,font_weight=600,color="onSurface"},
            tasksBody},
    }
end
