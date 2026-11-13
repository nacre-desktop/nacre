-- SPDX-License-Identifier: GPL-3.0-only
-- Pill segment control (design review D12). A stand-in for a kit `Segment` (see the N0239 kit
-- asks): a container-high pill holding one full-radius option each; the selected option is
-- filled primary. Settings enum rows use `Row`; Orient (D13) and the AI panel can use `Segment`.
--   options  {id, label, selected, enabled}  on_change  host action that receives the option id
return function(ui,kit)
    local sp=kit.tokens.spacing
    local tx=kit.tokens.text
    local M={}
    function M.Segment(p)
        local on=p.enabled~=false
        local size=tx.caption*4/3
        local height=sp.huge+sp.hairline
        local pressed=ui.resolve_for_check(ui.bind("bead.pressed"))
        local pill={id=p.id or "segment",height=height,shrink=0,padding=sp.xs,gap=sp.hairline,align="center",
            radius=kit.tokens.radii.full,fill=ui.alpha("surfaceContainerHigh",on and 1 or .4)}
        for i,o in ipairs(p.options) do
            local live=on and o.enabled~=false
            local base=o.selected and "primary" or "surfaceContainerHigh"
            local ink=o.selected and "onPrimary" or (live and "onSurfaceVariant" or "onSurface")
            local width=math.ceil(ui.text_width(o.label,"IBM Plex Sans",size))+2*sp.md+sp.hairline
            local option_h=height-2*sp.xs
            pill[#pill+1]=kit.Button{id="option-"..i,width=width,height=option_h,padding=0,padding_top=(option_h-20)/2,shrink=0,
                radius=kit.tokens.radii.full,selected=o.selected,justify="center",align="center",
                fill=o.selected and "primary" or "transparent",
                hover_fill=live and ui.tint(base,"onSurface",pressed and .12 or .08) or (o.selected and "primary" or "transparent"),
                aria_label=(p.aria_label or "")..": "..o.label,on_activate=live and ui.action(p.on_change,o.id) or nil,
                ui.Text{id="label",text=o.label,width=width,height=20,text_align="center",
                    font_size=size,font_weight=500,color=ui.alpha(ink,live and 1 or .4)}}
        end
        return ui.Row(pill)
    end
    -- Label and description on the left, the segment on the right (a `kit.ChoiceRow` shape).
    function M.Row(p)
        return kit.FormRow{id=p.id,label=p.label,description=p.description,enabled=p.enabled,
            M.Segment{id="options",options=p.options,enabled=p.enabled,aria_label=p.label,on_change=p.on_change}}
    end
    return M
end
