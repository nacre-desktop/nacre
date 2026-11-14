-- SPDX-License-Identifier: GPL-3.0-only
-- Colors app (B0060, design review D13): wallpaper colour explorer. Header (wordmark, page segment,
-- preview-mode segment) and a scrolling canvas. Studio is two columns: the wallpaper, its pigments as a
-- strand of pearls sized by area and the style chips on the left; a miniature of the bar, the three roles
-- named by their job and Export/Apply on the right. The wallpaper list is a picker that takes the image's
-- place while open. All data is plain projection from `colors.rs`; the miniature and role swatches draw
-- the previewed palette (RGBA from the host), everything else uses palette roles and tokens.
local ui=require("bead.ui")
local kit=require("bead.kit")
local bar=require("../bar/tokens")
local T=kit.tokens
local M=kit.metrics
local sp,rd=T.spacing,T.radii
local CAP,LABEL,BODY,TITLE=M.text.caption,M.text.label,M.text.body,M.text.title
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local function act(arg) return ui.action("colors.do",arg) end
local function lh(size) return math.ceil(size*1.35) end
local W=get("colors.win_w")
local page=get("colors.page")
local picker=get("colors.picker")==true
local MARGIN=sp.xxl
local GAP=sp.lg
local CANVAS=W-2*MARGIN
local split=CANVAS>=720
-- Review mockup: `1fr 300px` columns, 14 px apart.
local RIGHT=split and 300 or CANVAS
local LEFT=split and CANVAS-RIGHT-GAP or CANVAS
local function text(id,s,size,color,weight,width)
    return ui.Text{id=id,text=s,width=width or "fill",height=lh(size),shrink=0,font_size=size,font_weight=weight,color=color or "onSurface"}
end
local function measure(s,size) return math.ceil(ui.text_width(s,ui.default_font_family,size)) end
-- Wraps `kit.Chip`s by their measured width (the kit chip's own width formula).
local function chips(id,items,width)
    local rows,row,used={},nil,0
    for _,item in ipairs(items) do
        local w=2*sp.lg+measure(item.text,LABEL)+sp.hairline+(item.icon and T.icons.large*4/3+2+sp.sm or 0)
        if not row or used+sp.sm+w>width then
            row={id="line-"..(#rows+1),height=36,gap=sp.sm,shrink=0};rows[#rows+1]=row;used=-sp.sm
        end
        row[#row+1]=kit.Chip(item);used+=sp.sm+w
    end
    for i,r in ipairs(rows) do rows[i]=ui.Row(r) end
    rows.id=id;rows.width=width;rows.gap=sp.sm;rows.shrink=0
    return ui.Column(rows)
end
-- Preview mode: kit.Segment's track with icon-only options.
-- shortcut: kit.Segment has no glyph option (D13 kit ask 1); delete this when it does.
local function modes(v)
    local h=sp.huge+sp.hairline
    local w=T.icons.large*4/3+2*sp.md
    local pressed=get("bead.pressed")
    local pill={id="modes",width=2*sp.xs+2*w+sp.hairline,height=h,shrink=0,padding=sp.xs,gap=sp.hairline,align="center",
        radius=rd.full,fill="surfaceContainerHigh"}
    for _,m in ipairs{{"dark","dark_mode","bead.colors.dark-preview"},{"light","light_mode","bead.colors.light-preview"}} do
        local on=v.mode==m[1]
        local base=on and "primary" or "surfaceContainerHigh"
        pill[#pill+1]=kit.IconButton{id="mode-"..m[1],width=w,height=h-2*sp.xs,radius=rd.full,text=m[2],selected=on,
            font_size=T.icons.medium*4/3,fill=on and "primary" or "transparent",hover_fill=ui.tint(base,"onSurface",pressed and .12 or .08),
            color=on and "onPrimary" or "onSurfaceVariant",aria_label=ui.tr(m[3]),on_activate=act("mode:"..m[1])}
    end
    return ui.Row(pill)
end
local function header(v)
    local tabs={}
    for i,tab in ipairs{{"studio","bead.colors.studio"},{"compare","bead.colors.compare"},{"accessibility","bead.colors.accessibility"}} do
        tabs[i]={id="page:"..tab[1],label=ui.tr(tab[2]),selected=page==tab[1]}
    end
    local title=ui.tr("bead.colors.title")
    return ui.Row{id="header",width="fill",height=64,shrink=0,align="center",gap=sp.md,padding_left=MARGIN,padding_right=MARGIN,
        ui.Image{id="logo",path="colors/assets/nacre-colors.themed.svg",width=T.icons.xlarge,height=T.icons.xlarge,shrink=0,aria_label=title},
        ui.Text{id="title",text=title,width=measure(title,TITLE+1)+sp.xs,height=lh(TITLE+1),shrink=0,font_size=TITLE+1,font_weight=600},
        ui.Box{id="space",grow=1,height=1},
        kit.Segment{id="tabs",options=tabs,aria_label=title,on_change="colors.do"},
        ui.Box{id="space-2",grow=1,height=1},
        modes(v)}
end
-- Wallpaper picker cell: thumbnail over a one-line name.
local function picker_view(width,height)
    local inner=width-2*sp.md
    local search=40
    local cols=math.max(1,math.floor((inner+sp.sm)/(150+sp.sm)))
    local cell=(inner-(cols-1)*sp.sm)/cols
    local thumb=math.floor((cell-2*sp.xs)*.56)
    local function entry(item)
        return kit.Button{id="entry",width="100%",height="100%",padding=sp.xs,radius=M.radius.row,
            fill=item.selected and ui.alpha("primary",.14) or "transparent",hover_fill=ui.alpha("onSurface",.06),
            border_width=item.selected and 1 or 0,border_color=ui.alpha("primary",.65),aria_label=item.name..", "..item.kind,on_activate=act(item.action),
            ui.Column{id="body",width="fill",height="100%",gap=sp.xs,
                item.thumb~="" and ui.Image{id="thumb",width=cell-2*sp.xs,height=thumb,radius=sp.sm,shrink=0,icon=item.thumb,aria_label=item.name}
                    or ui.Box{id="thumb",width=cell-2*sp.xs,height=thumb,radius=sp.sm,shrink=0,fill="surfaceContainerHigh"},
                kit.Paragraph{id="name",text=item.name,width=cell-2*sp.xs,font_size=CAP,max_lines=1,color="onSurface"}}}
    end
    return ui.Column{id="picker",width=width,height=height,shrink=0,padding=sp.md,gap=sp.sm,radius=M.radius.tile,fill="surfaceContainer",
        kit.TextInput{id="search",text=ui.bind("colors.query"),placeholder=ui.tr("bead.colors.find-wallpaper"),width="fill",height=search,
            aria_label=ui.tr("bead.colors.find-wallpaper")},
        ui.Text{id="empty",visible=ui.bind("colors.empty"),text=ui.tr("bead.colors.no-matches"),width="fill",height=lh(CAP),font_size=CAP,color="onSurfaceVariant"},
        kit.VirtualGrid{id="entries",scroll=ui.bind("colors.rail.scroll"),width=inner,height=height-2*sp.md-search-sp.sm,model=ui.bind("colors.entries"),
            key="id",min_cell_width=150,row_height=thumb+2*sp.xs+sp.xs+lh(CAP),gap_x=sp.sm,gap_y=sp.sm,render=entry}}
end
-- The image's colour families as a strand of pearls, diameter by area (18 px at none .. 28 px at the
-- largest), overlapping like the bar's stack strand; each pearl is the sample button.
local function strand(samples)
    local most=0
    for _,s in ipairs(samples) do most=math.max(most,s.coverage or 0) end
    local overlap=bar.strand_overlap
    local kids,x={},0
    for i,s in ipairs(samples) do
        local d=most>0 and math.floor(18+10*math.sqrt((s.coverage or 0)/most)+.5) or bar.pearl_occupied
        local armed=s.action and s.action~=""
        kids[i]=kit.Button{id=s.id,position="absolute",left=x,top=(28-d)/2,width=d+4,height=d+4,padding=0,radius=(d+4)/2,
            fill=s.color,hover_fill=s.color,border_width=2,border_color="surface",aria_label=s.aria_label,
            on_activate=armed and act(s.action) or nil,ui.Box{id="hit",width=d+4,height=d+4}}
        x+=d+4-overlap
    end
    kids.id="strand";kids.width=x+overlap;kids.height=32;kids.shrink=0
    return ui.Box(kids),kids.width
end
-- Miniature of the bar in the previewed palette, from the bar's own tokens at 70 %: the workspace pill
-- with a current, an occupied and an empty pearl joined by the trace (accent, shell, text in order),
-- and the status pill with shell-coloured icons and the text-coloured clock.
local function mini_bar(colors,width)
    local k=.7
    local function c(role) return colors[role] or colors.onSurface or {232,232,232,255} end
    local pill=math.floor(bar.pill_height*k+.5)
    local number_size=math.floor(bar.workspace_number_font*k+.5)
    local function pearl(id,d,fill,ring,number,ink)
        return ui.Row{id=id,width=d,height=d,radius=d/2,shrink=0,align="center",justify="center",fill=fill or "transparent",
            border_width=ring and bar.pearl_empty_border or 0,border_color=ring,
            number and ui.Text{id="n",text=number,width=d,height=lh(number_size),font_size=number_size,font_weight=600,text_align="center",color=ink} or nil}
    end
    local function trace(id) return ui.Box{id=id,width=sp.sm,height=1,shrink=0,fill=c("secondary")} end
    local icon=math.floor(bar.icon_font*k+.5)
    local function glyph(id,name) return ui.Text{id=id,text=name,family=T.fonts.icon,width=icon,height=icon+2,shrink=0,font_size=icon,color=c("secondary")} end
    local clock=ui.tr("bead.colors.sample-time")
    local clock_size=math.floor(bar.clock_font*k+.5)
    return ui.Row{id="mini-bar",width=width,height=bar.pill_height,shrink=0,align="center",justify="space-between",padding_left=sp.sm,padding_right=sp.sm,
        radius=M.radius.row,fill=c("frame"),border_width=1,border_color=c("outline"),aria_label=ui.tr("bead.colors.bar-preview"),
        ui.Row{id="workspaces",height=pill,shrink=0,align="center",padding_left=sp.xs,padding_right=sp.xs,radius=pill/2,fill=c("surfaceContainer"),
            pearl("current",math.floor(bar.pearl_current*k+.5),c("primary"),nil,"1",c("onPrimary")),trace("trace-1"),
            pearl("occupied",math.floor(bar.pearl_occupied*k+.5),c("secondary"),nil,"2",c("onSecondary")),trace("trace-2"),
            pearl("empty",math.floor(bar.pearl_empty*k+.5),nil,c("tertiary"))},
        ui.Row{id="status",height=pill,shrink=0,align="center",gap=sp.sm,padding_left=sp.sm,padding_right=sp.sm,radius=pill/2,fill=c("surfaceContainer"),
            glyph("wifi","wifi"),glyph("volume","volume_up"),
            ui.Text{id="clock",text=clock,width=measure(clock,clock_size)+2,height=lh(clock_size),shrink=0,font_size=clock_size,font_weight=500,color=c("tertiary")}}}
end
-- A role named by its job: the row arms it for the next picked colour, the lock pins it.
local function role_row(t)
    return ui.Row{id="role-"..t.id,width="fill",height=56,shrink=0,align="center",radius=M.radius.row,fill="surfaceContainer",
        border_width=t.armed and 2 or 0,border_color="primary",
        kit.Button{id="main",grow=1,height=56,padding=0,padding_left=sp.md,radius=M.radius.row,fill="transparent",hover_fill=ui.alpha("onSurface",.06),
            aria_label=t.label.." "..t.value,on_activate=act(t.arm),
            ui.Row{id="main-row",grow=1,height=56,gap=sp.md,align="center",
                ui.Column{id="labels",grow=1,height=lh(BODY)+lh(CAP),gap=0,
                    text("label",t.label,BODY,"onSurface",500),
                    text("detail",t.detail,CAP,"onSurfaceVariant")},
                ui.Box{id="swatch",width=26,height=26,radius=13,shrink=0,fill=t.color,border_width=1,border_color=ui.alpha("outline",.5)}}},
        kit.IconButton{id="lock",text=t.locked and "lock" or "lock_open",aria_label=t.lock_label,width=36,height=36,radius=rd.full,
            font_size=T.icons.medium*4/3,fill="transparent",hover_fill=ui.alpha("onSurface",.08),color=t.locked and "onSurface" or "onSurfaceVariant",
            on_activate=act(t.lock)}}
end
-- Status line over Export and Apply, right-aligned where the eye ends.
local function actions(v,width)
    local apply=v.applied and ui.tr("bead.colors.in-use") or ui.tr("bead.colors.apply")
    local export=ui.tr("bead.colors.export")
    local size=T.text.caption*4/3
    return ui.Column{id="actions",width=width,gap=sp.sm,shrink=0,
        ui.Text{id="status",text=v.status,visible=v.status~="",width="fill",height=lh(CAP),font_size=CAP,text_align="right",
            color=v.status_error and "error" or "onSurfaceVariant"},
        ui.Row{id="buttons",width="fill",height=34,gap=sp.sm,align="center",
            ui.Box{id="space",grow=1,height=1},
            kit.PillButton{id="export",compact=true,glyph="upload",label=export,label_width=ui.text_width(export,ui.default_font_family,size),
                on_activate=v.can_export and act("export") or nil},
            kit.PillButton{id="apply",compact=true,glyph="check",label=apply,label_width=ui.text_width(apply,ui.default_font_family,size),selected=true,
                on_activate=v.can_apply and act("apply") or nil}}}
end
local function fine_tune(v,width)
    local out={id="fine",width=width,gap=sp.compact,shrink=0,
        text("tint-title",ui.tr("bead.colors.background-tint"),BODY,"onSurface",500),
        chips("tint",{{id="tint-accent",text=ui.tr("bead.colors.tint-accent"),on=not v.tint_wallpaper,on_activate=act("bg:accent")},
            {id="tint-wallpaper",text=ui.tr("bead.colors.tint-wallpaper"),on=v.tint_wallpaper,on_activate=v.tint_wallpaper_enabled and act("bg:wallpaper") or nil}},width),
        kit.Paragraph{id="tint-note",text=v.tint_wallpaper and ui.tr("bead.colors.tint-wallpaper-note") or ui.tr("bead.colors.tint-accent-note"),
            width=width,font_size=CAP,color="onSurfaceVariant"},
        text("brightness-title",ui.tr("bead.colors.brightness"),BODY,"onSurface",500),
        kit.Slider{id="brightness",width=width,min=-.15,max=.15,step=.01,value=v.brightness,label=ui.tr("bead.colors.brightness"),
            on_change="colors.do",change_arg=function(n) return string.format("brightness:%.3f",n) end},
        text("strength-title",ui.tr("bead.colors.strength"),BODY,"onSurface",500),
        kit.Slider{id="strength",width=width,min=0,max=1.6,step=.05,value=v.saturation,label=ui.tr("bead.colors.strength"),
            on_change="colors.do",change_arg=function(n) return string.format("saturation:%.3f",n) end},
        kit.ToggleRow{id="time-tint",label=ui.tr("bead.colors.time-shift"),checked=v.time_tint,on_activate=act("tint")},
        kit.ToggleRow{id="ws-colors",label=ui.tr("bead.colors.workspace-chips"),checked=v.workspace_colors,on_activate=act("wsc")},
        chips("reset-row",{{id="reset",icon="restart_alt",text=ui.tr("bead.colors.reset"),on_activate=act("reset")}},width)}
    return ui.Column(out)
end
local function studio(v)
    local ph=math.floor(LEFT*.5)
    local stage
    if picker then
        stage=picker_view(LEFT,ph)
    elseif v.has_photo then
        stage=kit.ImagePick{id="photo",width=LEFT,height=ph,icon="colors-photo",cols=16,rows=10,on_pick="colors.do",
            pick_arg=function(x,y) return string.format("pick:%.4f,%.4f",x,y) end,aria_label=ui.tr("bead.colors.pick-hint")}
    else
        stage=ui.Box{id="photo",width=LEFT,height=ph,radius=M.radius.tile,fill="surfaceContainerLow",shrink=0}
    end
    local pearls,pearls_w=strand(v.samples)
    local wallpapers=ui.tr("bead.colors.wallpapers")
    -- kit.Chip's own width; a grow=1 text beside a fixed chip overflows the row, so the caption gets the rest.
    local chip_w=2*sp.lg+measure(wallpapers,LABEL)+sp.hairline+T.icons.large*4/3+2+sp.sm
    local title_cap=math.floor(LEFT*.45)
    local title_w=math.min(title_cap,measure(v.title,TITLE)+sp.xs)
    -- A long file name ends in an ellipsis instead of running under the subtitle.
    local title=title_w<title_cap and ui.Text{id="title",text=v.title,width=title_w,height=lh(TITLE),shrink=0,font_size=TITLE,font_weight=500}
        or kit.Paragraph{id="title",text=v.title,width=title_w,font_size=TITLE,max_lines=1}
    local styles={}
    for i,s in ipairs(v.styles) do
        styles[i]={id="style-"..s.id,text=s.label,on=s.selected,on_activate=v.available and act("style:"..s.id) or nil}
    end
    styles[#styles+1]={id="fine-toggle",icon="tune",text=v.fine and ui.tr("bead.colors.hide-fine-tuning") or ui.tr("bead.colors.fine-tune"),
        on=v.fine,on_activate=act("fine")}
    local left={id="left",width=LEFT,gap=sp.compact,shrink=0,
        stage,
        ui.Row{id="name-row",width="fill",height=36,gap=sp.md,align="center",shrink=0,
            title,
            ui.Text{id="subtitle",text=v.subtitle,width=math.max(0,LEFT-title_w-chip_w-2*sp.md),height=lh(LABEL),shrink=0,font_size=LABEL,color="onSurfaceVariant"},
            kit.Chip{id="wallpapers",icon="wallpaper",text=wallpapers,on=picker,on_activate=act("picker")}},
        ui.Row{id="pigments",width="fill",height=32,gap=sp.compact,align="center",shrink=0,
            pearls,
            ui.Text{id="coverage",text=v.coverage,width=math.max(0,LEFT-pearls_w-sp.compact),height=lh(LABEL),shrink=0,font_size=LABEL,color="onSurfaceVariant"}},
        chips("styles",styles,LEFT),
        kit.Paragraph{id="style-text",text=v.style_text,width=LEFT,font_size=CAP,color="onSurfaceVariant"}}
    if v.fine then table.insert(left,fine_tune(v,LEFT)) end
    local right={id="right",width=RIGHT,gap=sp.compact,shrink=0,mini_bar(v.colors,RIGHT)}
    for _,t in ipairs(v.targets) do right[#right+1]=role_row(t) end
    right[#right+1]=ui.Box{id="before-actions",width=1,height=sp.xs,shrink=0}
    right[#right+1]=actions(v,RIGHT)
    if split then
        return ui.Row{id="studio",width="fill",gap=GAP,shrink=0,ui.Column(left),ui.Column(right)}
    end
    return ui.Column{id="studio",width="fill",gap=sp.xl,shrink=0,ui.Column(left),ui.Column(right)}
end
local function card_labels()
    return {brand=ui.tr("bead.colors.card-brand"),files=ui.tr("bead.colors.card-files"),
        lines={ui.tr("bead.colors.card-documents"),ui.tr("bead.colors.card-pictures"),ui.tr("bead.colors.card-projects")},open=ui.tr("bead.colors.card-open"),
        build=ui.tr("bead.colors.card-build"),ready=ui.tr("bead.colors.card-ready"),warning=ui.tr("bead.colors.card-warning"),
        message=ui.tr("bead.colors.card-message"),message_detail=ui.tr("bead.colors.card-message-detail")}
end
local function compare(v)
    local w=math.max(300,(CANVAS-12)/2)
    local rows,row={},nil
    for _,c in ipairs(v.compare) do
        if not row or #row>=2 then row={};table.insert(rows,row) end
        table.insert(row,ui.Column{id="cmp-"..c.id,width=w,gap=8,shrink=0,
            text("name",c.label,BODY,"onSurface",500),
            ui.Text{id="contrast",text=c.contrast,visible=c.contrast~="",width="fill",height=c.contrast~="" and lh(CAP) or 0,font_size=CAP,color="onSurfaceVariant"},
            kit.PreviewCard{id="card",width=w,height=math.min(260,w*.6),colors=c.colors,image=v.has_photo and "colors-card" or "",
                aria_label=c.label,labels=card_labels()}})
    end
    local out={id="compare",width="fill",gap=14,shrink=0,text("compare-title",ui.tr("bead.colors.compare-styles"),TITLE,"onSurface",500)}
    for i,r in ipairs(rows) do
        r.id="pair-"..i;r.gap=12;r.shrink=0
        table.insert(out,ui.Row(r))
    end
    table.insert(out,actions(v,CANVAS))
    return ui.Column(out)
end
local function access(v)
    local vision={}
    for i,c in ipairs(v.vision) do vision[i]={id="vision:"..c.id,label=c.label,selected=c.selected} end
    local out={id="access",width="fill",gap=12,shrink=0,
        text("access-title",ui.tr("bead.colors.access-title"),TITLE,"onSurface",500),
        text("contrast",v.contrast,LABEL,"onSurfaceVariant"),
        kit.Paragraph{id="coverage",text=ui.tr("bead.colors.access-coverage"),width=CANVAS,font_size=LABEL,color="onSurfaceVariant"},
        kit.Segment{id="vision",options=vision,aria_label=ui.tr("bead.colors.access-title"),on_change="colors.do"},
        kit.PreviewCard{id="card",width=math.min(CANVAS,620),height=300,colors=v.vision_colors,image=v.has_photo and "colors-card" or "",
            aria_label=ui.tr("bead.colors.preview-aria"),labels=card_labels()}}
    for _,w in ipairs(v.warnings) do table.insert(out,text(w.id,w.text,LABEL,"onSurfaceVariant")) end
    for _,r in ipairs(v.rows) do table.insert(out,text(r.id,r.text,CAP,"onSurfaceVariant")) end
    table.insert(out,actions(v,CANVAS))
    return ui.Column(out)
end
local function canvas(v)
    local body={id="body",width="fill",gap=14,padding_left=MARGIN,padding_right=MARGIN,padding_bottom=MARGIN}
    if page=="compare" or page=="accessibility" then
        body[1]=text("title",v.title,TITLE+4,"onSurface",500)
        body[2]=text("subtitle",v.subtitle,LABEL,"onSurfaceVariant")
        body[3]=(page=="compare" and compare or access)(v)
    else
        body[1]=studio(v)
    end
    return ui.Scroll{id="canvas",scroll=ui.bind("colors.scroll"),grow=1,ui.Column(body)}
end
return ui.Column{id="colors",width="100%",height="100%",fill="surface",
    ui.Repeat{id="top",model=ui.bind("colors.view"),key="id",render=header},
    ui.Column{id="content",grow=1,width="fill",
        ui.Repeat{id="page",model=ui.bind("colors.view"),key="id",render=canvas}}}
