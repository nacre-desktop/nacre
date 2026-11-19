-- SPDX-License-Identifier: GPL-3.0-only
local base_ui=require("bead.ui")
local ui=base_ui
local samples=base_ui.resolve_for_check(base_ui.bind("widgets.preview"))
local sample=samples and samples[1]
if sample then
    ui={}
    for key,value in base_ui do ui[key]=value end
    ui.bind=function(key)
        local value=sample[key]
        if type(value)=="table" and value.fallback then
            local binding=base_ui.bind(key)
            local current=base_ui.resolve_for_check(binding)
            return current~=nil and current~="" and binding or value.fallback
        end
        return value~=nil and value or base_ui.bind(key)
    end
end
local kit=require("bead.kit")
local t=require("../bar/tokens")
local function centered_text(p)
    local lift=p.family=="Material Symbols Rounded" and 0 or t.text_lift
    p.height=p.font_size*t.line_height+2*lift
    if p.position=="absolute" then p.top=(p.top or 0)-lift end
    return ui.Text(p)
end
local launcher=ui.resolve_for_check(ui.bind("bar.launcher"))
local popouts=ui.resolve_for_check(ui.bind("bar.popouts"))
local ws_on=ui.resolve_for_check(ui.bind("bar.workspace.enabled"))
local tray_on=ui.resolve_for_check(ui.bind("bar.tray.enabled"))
local dash_on=ui.resolve_for_check(ui.bind("bar.dashboard.enabled"))
local focus_on=ui.resolve_for_check(ui.bind("bar.window.focus.enabled"))
local updates_on=ui.resolve_for_check(ui.bind("bar.updates.enabled"))
local updates_n=ui.resolve_for_check(ui.bind("bar.updates.count"))
local badges_on=ui.resolve_for_check(ui.bind("bar.status.badges"))~=false
local header=ui.resolve_for_check(ui.bind("bar.height"))
-- Lacquer surfaces (locked 2026-11-17). Screen border "none": islands float on a transparent bar over a thread,
-- opaque "surface" with a 1 px rim, a lit top edge and a short contact shadow (B0320: tight, 13 px margin, the
-- surface clips it at the bar zone). The frame variant keeps its pills as the "raised" role, with the same lit edge.
local islands=ui.resolve_for_check(ui.bind("bar.frame"))==false
local base=islands and "surface" or "raised"
local i_fill=islands and "surface" or nil
local i_stroke=islands and 1 or nil
local i_stroke_color=islands and ui.alpha("onSurface",.05) or nil
local i_shadow=islands and {blur=4,spread=0,dy=1,alpha=.6} or nil
-- The lit edge: surfaceBright is a soft lift on dark lacquer and near white on the light one; a straight 1 px
-- line inside the rounded top. unshaped or transparent pieces get none.
local function lit_edge(radius,visible)
    return ui.Box{id="lit-edge",position="absolute",left=radius,right=radius,top=0,height=1,visible=visible,fill=ui.alpha("surfaceBright",.6)}
end
local function island(p)
    if islands then p.fill,p.border_width,p.border_color,p.shadow=i_fill,i_stroke,i_stroke_color,i_shadow end
    if p.radius and p.fill~="transparent" then p[#p+1]=lit_edge(p.radius,true) end
    return p
end
-- The title band: 30 px centred in the frame bar; a 34 px island 6 px from the top in the islands bar.
local band_top=islands and 6 or (header-t.title_height)/2
local band_height=islands and t.pill_height or t.title_height
local title=ui.bind("bar.title")
local title_width=math.min(t.title_max_width,math.ceil(ui.text_width(title,"IBM Plex Sans",t.title_font))+kit.tokens.spacing.hairline/2)
local cue_extent=math.min(t.title_cue_max,title_width+t.icon_width+t.title_gap+10)
local dwell=ui.resolve_for_check(ui.bind("bead.edge.dwell"))
local ws_count=ui.resolve_for_check(ui.bind("bar.workspace.count"))
local ws_icons=ui.resolve_for_check(ui.bind("bar.workspace.icons"))
local thread=true
for _,entry in ws_icons do if entry.icon~="" then thread=false end end
-- QML NacreWorkspaceRow.tint: pearls blend primary -> secondary -> tertiary by position along the row.
local function pearl_accent(at)
    local position=math.min(1,at/math.max(1,ws_count-1))
    if position<=0.5 then return ui.tint("primary","secondary",position*2) end
    return ui.tint("secondary","tertiary",(position-0.5)*2)
end
local function glyph(id,text,color)
    return centered_text{id=id,text=text,width=t.icon_width,shrink=0,family="Material Symbols Rounded",font_size=t.icon_font,color=color or "secondary",text_align="center"}
end
-- D1 (spec 0237): one 20 px glyph in a 24 px box in t.status_tone whatever its state (owner 2026-11-14);
-- `color` overrides it for attention states (warn/error/success). `badge` is a 15 px error
-- count pill at the box's top-right corner (hidden when empty or when Settings turns badges off);
-- `kind` is kept for callers' ids.
local function status_glyph(id,text,_kind,color,badge,_active)
    local box=t.icon_font*t.line_height
    local count=badge and badges_on and tostring(ui.resolve_for_check(badge) or "") or ""
    local badge_width=count~="" and math.max(t.badge_size,math.ceil(ui.text_width(count,"IBM Plex Sans",t.badge_font))+2*t.badge_pad) or 0
    return ui.Row{id=id,width=t.icon_width,height=box,shrink=0,align="center",justify="center",
        ui.Text{id="ink",width=t.icon_width,height=box,shrink=0,
            text=text,family="Material Symbols Rounded",font_size=t.icon_font,color=color or t.status_tone,text_align="center"},
        count~="" and ui.Box{id="badge",position="absolute",left=t.icon_width+t.badge_overhang-badge_width,top=(box-t.icon_font)/2-t.badge_lift,width=badge_width,height=t.badge_size,radius=t.badge_size/2,fill="error",
            ui.Text{id="count",text=count,width=badge_width,height=t.badge_size,font_size=t.badge_font,font_weight=600,family="IBM Plex Sans",text_align="center",color="onError"}} or nil}
end
-- Layout snaps every box to whole logical pixels, and text paints wrapped at its box's
-- device width. A label box sized to its exact fractional advance can round below the
-- glyph run and wrap the last glyph out of its one line ("34" drew as "3" at scale 1),
-- so label boxes take whole pixels. The tolerance only absorbs animation rounding noise.
local function whole_width(width)
    return type(width)=="number" and math.ceil(width-1e-9) or width
end
local function status_text(p)
    -- Text paints from the top of its line box; centering a shorter box lowers
    -- the baseline without changing the group's hit area or the icon's centre.
    local offset=p.font_size==t.clock_font and t.clock_text_offset or t.status_text_offset
    p.height=p.font_size*t.line_height-2*offset
    p.family="IBM Plex Sans"
    p.width=whole_width(p.width)
    p.max_width=whole_width(p.max_width)
    return ui.Text(p)
end
return {lit_edge=lit_edge,i_shadow=i_shadow,i_fill=i_fill,i_stroke=i_stroke,i_stroke_color=i_stroke_color,islands=islands,base=base,island=island,band_top=band_top,band_height=band_height,ui=ui,kit=kit,t=t,centered_text=centered_text,launcher=launcher,popouts=popouts,ws_on=ws_on,tray_on=tray_on,dash_on=dash_on,focus_on=focus_on,updates_on=updates_on,updates_n=updates_n,header=header,title=title,title_width=title_width,cue_extent=cue_extent,dwell=dwell,ws_count=ws_count,ws_icons=ws_icons,thread=thread,pearl_accent=pearl_accent,glyph=glyph,status_glyph=status_glyph,status_text=status_text,whole_width=whole_width}
