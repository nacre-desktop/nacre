-- SPDX-License-Identifier: GPL-3.0-only
-- AI usage (design review D9): one ring per quota window the provider reports (`percent` rows;
-- primary for the first provider, secondary for the rest, the reset countdown as the caption),
-- plain lines for providers without structured windows, refresh as the card action. Nothing is
-- estimated: a provider without windows shows only its text.
local ui=require("bead.ui")
local kit=require("bead.kit")
local empty=require("../../ui/components/empty-state")
local T=kit.tokens
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local rows=ui.resolve_for_check(ui.bind("dashboard.rows"))
    local W,H,PAD=940,562,T.spacing.md+T.spacing.xs
    local INNER=W-2*PAD
    local HEAD,RING,STROKE=36,96,10
    local BODY,CAPTION=T.text.body*4/3,T.text.caption*4/3
    local refresh=control and ui.action("dashboard.do","usage.refresh") or nil
    local rings,lines={},{}
    local tone,provider={},{}
    for _,item in ipairs(rows) do
        if item.provider and item.provider~="" and not tone[item.provider] then
            tone[item.provider]=#provider==0 and "primary" or "secondary"
            table.insert(provider,item.provider)
        end
        if type(item.percent)=="number" then
            local fraction=math.clamp(item.percent/100,0,1)
            local ink=tone[item.provider] or "primary"
            local captionLines=item.detail~="" and 2 or 1
            table.insert(rings,ui.Box{id="ring-"..item.id,width=RING+T.spacing.huge,height=RING+T.spacing.sm+captionLines*18,
                ui.Arc{id="track",position="absolute",left=T.spacing.md+STROKE/2,top=STROKE/2,width=RING-STROKE,height=RING-STROKE,
                    start_angle=-90,sweep_angle=360,stroke_width=STROKE,color="surfaceContainerHigh"},
                ui.Arc{id="amount",visible=fraction>0,position="absolute",left=T.spacing.md+STROKE/2,top=STROKE/2,width=RING-STROKE,height=RING-STROKE,
                    start_angle=-90,sweep_angle=360*fraction,stroke_width=STROKE,color=ink,round_caps=true},
                ui.Text{id="percent",position="absolute",left=T.spacing.md,top=(RING-26)/2,width=RING,height=26,
                    text=item.value,font_size=20,font_weight=600,text_align="center",color="onSurface"},
                ui.Text{id="label",position="absolute",left=0,top=RING+T.spacing.sm,width=RING+T.spacing.huge,height=18,
                    text=item.label,font_size=CAPTION,text_align="center",color="onSurface"},
                item.detail~="" and ui.Text{id="resets",position="absolute",left=0,top=RING+T.spacing.sm+18,width=RING+T.spacing.huge,height=18,
                    text=item.detail,font_size=CAPTION,text_align="center",color="onSurfaceVariant"} or nil})
        else
            table.insert(lines,ui.Text{id="line-"..item.id,text=item.label,width=INNER,height=22,font_size=item.head and BODY or CAPTION,
                font_weight=item.head and 500 or 400,color=item.head and "onSurface" or "onSurfaceVariant"})
        end
    end
    local body
    if #rows==0 then
        body=empty{top=PAD+HEAD,width=INNER,height=H-PAD*2-HEAD,glyph="data_usage",title=ui.tr("bead.dashboard.ai-usage-none"),
            detail=ui.resolve_for_check(ui.bind("dashboard.detail")),chip=ui.tr("bead.dashboard.refresh"),chip_glyph="refresh",on_chip=refresh}
    else
        local ringsTop=PAD+HEAD+T.spacing.xxl
        local col={id="usage",position="absolute",left=PAD,top=ringsTop,width=INNER,height=H-ringsTop-PAD,gap=T.spacing.xxl}
        if #rings>0 then col[#col+1]=ui.Row{id="rings",width=INNER,height=RING+T.spacing.sm+36,gap=T.spacing.huge,justify="center",table.unpack(rings)} end
        if #lines>0 then col[#col+1]=ui.Column{id="lines",width=INNER,gap=T.spacing.xs,table.unpack(lines)} end
        body=ui.Column(col)
    end
    return ui.Box{id="ai-usage",width=W,height=H,radius=T.spacing.xl,fill="surfaceContainer",
        ui.Row{id="header",position="absolute",left=PAD,top=PAD,width=INNER,height=HEAD,justify="space-between",align="center",
            ui.Text{id="title",text=ui.bind("dashboard.heading"),width=INNER-HEAD-T.spacing.md,height=24,font_size=BODY,font_weight=600,color="onSurface"},
            kit.IconButton{id="refresh",text="refresh",aria_label=ui.tr("bead.dashboard.refresh"),width=HEAD,height=HEAD,padding=0,radius=T.radii.full,
                font_size=T.icons.large*4/3,fill="transparent",hover_fill=ui.alpha("onSurface",.08),color=refresh and "onSurface" or "onSurfaceVariant",
                on_activate=refresh}},
        body,
    }
end
