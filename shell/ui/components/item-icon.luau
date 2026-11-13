-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local glyphs=require("./glyphs")
local tone=require("./icon-tone")
return function(item,size)
    -- No icon at all: a tinted circle with the name's initial in primary, like the pearls (D11).
    if item.icon=="" then
        local font=math.floor(size*.4)
        local line=math.ceil(ui.text_line_height("IBM Plex Sans",font))
        return ui.Column{id="icon",width=size,height=size,shrink=0,radius=size/2,justify="center",align="center",
            fill=ui.tint("surfaceContainer","primary",.12),
            ui.Text{text=item.name~="" and string.upper(utf8.char(utf8.codepoint(item.name,1))) or "?",aria_label=item.name,
                width=size,height=line,font_size=font,font_weight=600,text_align="center",color="primary"}}
    end
    -- Desktop entries may use the same name as a symbol; preserve their theme icon.
    if item.kind~="app" and glyphs[item.icon] then
        return ui.Text{id="icon",text=item.icon,family="Material Symbols Rounded",font_size=size,
            width=size,height=size,shrink=0,color=tone(),aria_label=item.name}
    end
    return ui.Image{id="icon",icon=item.icon,width=size,height=size,aria_label=item.name}
end
