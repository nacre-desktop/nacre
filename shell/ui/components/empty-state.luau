-- SPDX-License-Identifier: GPL-3.0-only
-- Shared empty state (design review D9 page grammar): a hero-size outline glyph, one line in
-- onSurface, one line in onSurfaceVariant and at most one chip. Callers pass strings and a
-- ready-made action; sizes come from the kit tokens. Centred in `width` x `height` at
-- (`left`, `top`) of an absolutely positioned parent.
--   {id?, left?, top?, width, height, glyph, title, detail?, chip?, chip_glyph?, on_chip?}
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
return function(p)
    local glyph=T.icons.hero*4/3
    local body,caption=T.text.body*4/3,T.text.caption*4/3
    local height=glyph+T.spacing.sm+22
    local col={id=p.id or "empty",position="absolute",left=p.left or 0,width=p.width,gap=T.spacing.sm,align="center",
        ui.Text{id="glyph",text=p.glyph,family=T.fonts.icon,font_size=glyph,width=glyph,height=glyph,text_align="center",color="outline"},
        ui.Text{id="title",text=p.title,width=p.width,height=22,font_size=body,font_weight=500,text_align="center",color="onSurface"}}
    if p.detail and p.detail~="" then
        height+=T.spacing.sm+18
        col[#col+1]=ui.Text{id="detail",text=p.detail,width=p.width,height=18,font_size=caption,text_align="center",color="onSurfaceVariant"}
    end
    if p.chip then
        height+=T.spacing.sm+34
        col[#col+1]=kit.PillButton{id="chip",compact=true,label=p.chip,glyph=p.chip_glyph,
            label_width=ui.text_width(p.chip,"IBM Plex Sans",caption),on_activate=p.on_chip}
    end
    col.height=height
    col.top=(p.top or 0)+math.max(0,(p.height-height)/2)
    return ui.Column(col)
end
