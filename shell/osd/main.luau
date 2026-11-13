-- SPDX-License-Identifier: GPL-3.0-only
-- On-screen display (B0051, design review D10): one silhouette for every `osd` kind, a pill with the glyph in a
-- 36 px circle, the title over the gradient level bar (or over its detail), the value on the right.
-- The host projects `osd.kind/value/muted/label/device`; kinds.luau holds what each kind looks like.
-- 336 x 60 (radius 30), not the review's 360 x 64: that is the shape Bead prewarms and caches the OSD shadow for
-- (B0156; a cold shadow raster at 1.5x cost a dropped first frame); the review's size waits on N0243 kit ask 4.
local ui=require("bead.ui")
local kit=require("bead.kit")
local measure=require("bead.widgets.measure")
local kinds=require("./kinds")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local SANS="IBM Plex Sans"
local W,H,PAD_L,PAD_R,GAP,CIRCLE=336,60,16,20,T.spacing.lg,T.spacing.huge+T.spacing.xs
local TITLE,DETAIL,VALUE,GLYPH=(T.text.caption+1)*4/3,12,T.text.body*4/3,T.icons.medium*4/3
local LINE_H,DETAIL_H,METER_H,GLYPH_H=20,16,8,math.ceil(GLYPH*1.4)
local TEXT_X=PAD_L+CIRCLE+GAP
-- `ui.text_width` measures the regular face; the 600 face runs about 6% wider.
local BOLD=1.06
local kind,value,muted,label,device=get("osd.kind"),get("osd.value"),get("osd.muted"),get("osd.label"),get("osd.device")
local spec=kinds[kind] or kinds.volume
value=math.clamp(value,0,1)
local function glyph(g)
    if g.muted and muted then return g.muted end
    for _,step in g.below or {} do if value<step[1] then return step[2] end end
    local hay=string.lower(label.." "..device)
    for _,m in g.match or {} do for _,p in m.patterns do if string.find(hay,p,1,true) then return m.glyph end end end
    return g.by_label and g.by_label[label] or g.default
end
local function detail(d)
    if d.state then return ui.tr(value>0 and "bead.osd.on" or "bead.osd.off") end
    if d.fixed then return ui.tr(d.fixed) end
    if d.by_label and d.by_label[label] then return ui.tr(d.by_label[label]) end
    if d.label and label~="" then return label end
    if d.fallback then return ui.tr(d.fallback) end
    return ""
end
local level,text=spec.level==true,detail(spec.detail)
-- A pill that appears shows its level already filled: while hidden, and on the first frame after
-- the show (`osd-shown` has not left 0 yet), the fill snaps to the value; only changes made while
-- the pill is visible animate (design review: the bar value tweens 120 ms).
local shown=ui.animate("osd-shown",get("osd.open") and 1 or 0,0.001,"linear")
local fill_value=level and ui.animate("osd-level",value,shown>=1 and 0.12 or 0,"out-cubic") or value
local dim=muted or (spec.toggle==true and value<=0)
local tint=dim and "onSurfaceVariant" or (spec.tint and spec.tint[label]) or "primary"
local number=muted and ui.tr("bead.osd.muted") or tostring(math.floor(value*100+.5))
-- The value column always fits "100", so the bar keeps its length while the level moves.
local value_w=level and math.ceil(math.max(ui.text_width("100",SANS,VALUE),ui.text_width(number,SANS,VALUE))*BOLD) or 0
local text_w=W-PAD_R-TEXT_X-(level and value_w+GAP or 0)
local title=measure.ellipsize(ui.tr(spec.title),text_w/BOLD,SANS,TITLE)
local title_w=math.min(text_w,math.ceil(ui.text_width(title,SANS,TITLE)*BOLD))
-- A level kind reads "Title · detail" on one line over the bar; the others put the detail under the title.
local inline_w=math.max(0,text_w-title_w-T.spacing.hairline)
local inline=level and text~="" and measure.ellipsize("· "..text,inline_w,SANS,DETAIL) or ""
-- An inline detail cut to a stub ("· Keybo…") says nothing: drop it.
if string.sub(inline,-3)=="…" and (utf8.len(inline) or 0)<11 then inline="" end
local top=level and (H-LINE_H-T.spacing.xs-METER_H)/2 or (H-LINE_H-DETAIL_H)/2
-- Motion (design review "Timings for 120 Hz", OSD): 160 ms in, rising 12 px with the fade; 200 ms out.
-- D16: the kit opens with its EaseOut (out-cubic) and closes with its accelerate curve, at most .75 x the open
-- duration (120 ms), until the B0263 easing tokens land.
return kit.Panel{edge="bottom",id="osd",width=W,height=H,on_activate=ui.action("notifications.osd",true),open=ui.bind("panel.open"),duration=.16,return_duration=.2,slide_distance=12,
    fill="surfaceContainer",radius=H/2,border_width=1,border_color=ui.alpha("outline",.28),padding=0,
    ui.Box{id="glyph",position="absolute",left=PAD_L,top=(H-CIRCLE)/2,width=CIRCLE,height=CIRCLE,radius=CIRCLE/2,fill="surfaceContainerHigh"},
    ui.Text{id="icon",position="absolute",left=PAD_L,top=(H-GLYPH_H)/2,width=CIRCLE,height=GLYPH_H,text=glyph(spec.glyph),family="Material Symbols Rounded",font_size=GLYPH,text_align="center",color=tint},
    ui.Text{id="title",position="absolute",left=TEXT_X,top=top,width=title_w,height=LINE_H,family=SANS,text=title,font_size=TITLE,font_weight=600},
    ui.Text{id="detail",position="absolute",left=level and TEXT_X+title_w+T.spacing.hairline or TEXT_X,top=level and top+T.spacing.hairline or top+LINE_H,
        width=level and inline_w or text_w,height=DETAIL_H,family=SANS,text=level and inline or text,font_size=DETAIL,color="onSurfaceVariant",visible=(level and inline or text)~=""},
    ui.Text{id="value",position="absolute",left=W-PAD_R-value_w,top=(H-LINE_H)/2,width=value_w,height=LINE_H,family=SANS,text=level and number or "",text_align="right",font_size=VALUE,font_weight=600,
        color=muted and "onSurfaceVariant" or "onSurface",visible=level},
    ui.Box{id="track",position="absolute",left=TEXT_X,top=top+LINE_H+T.spacing.xs,width=text_w,height=METER_H,radius=METER_H/2,fill="surfaceContainerHighest",visible=level,
        ui.Image{id="fill",path="osd/assets/progress.themed.svg",width=text_w*fill_value,height=METER_H,visible=not muted},
        ui.Box{id="muted-fill",width=text_w*fill_value,height=METER_H,radius=METER_H/2,fill=ui.alpha("primary",.35),visible=muted}},
}
