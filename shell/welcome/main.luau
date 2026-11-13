-- SPDX-License-Identifier: GPL-3.0-only
-- Welcome (B0059): the first-run tour as an on-demand window composed from kit components.
-- Pages, app tiles, help and links are data in `catalog.luau`; the adapter only supplies the window
-- size, which apps this desktop can open and the shortcut rows. Text blocks wrap inside their
-- box, so heights come from `textH`, an estimate from the character count and the content width.
local ui=require("bead.ui")
local kit=require("bead.kit")
local catalog=require("./catalog")
local T=kit.tokens
local sp,tx,rd,ic=T.spacing,T.text,T.radii,T.icons
local M,motion=kit.metrics,ui.motion
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local function px(pt) return pt*4/3 end
local function act(arg) return ui.action("welcome.do",arg) end
local function go(name) return ui.action("go",name) end

local page,guide,from=get("welcome.step"),get("welcome.guide"),get("welcome.from")
local W,H=get("welcome.width"),get("welcome.height")
-- Content box: the window minus 24 px side margins; the canvas also loses the header and footer.
local CW=W-2*sp.xxl
local CH=H-176

local function wrapped(s,size,width)
    return math.max(1,math.ceil(#s/math.max(1,math.floor(width/(size*0.48)))))
end
local function textH(s,size,width)
    return math.ceil(wrapped(s,size,width)*size*1.4)+2
end
-- Visual height of a block (the box is taller by the line pitch): one line is about 1.25 em in QML.
local function inkH(s,size,width)
    local n=wrapped(s,size,width)
    return n==1 and math.ceil(size*1.25) or math.ceil(n*size*1.4)
end
-- A text block wrapped at `o.width` (the content width by default).
local function para(id,s,pt,o)
    o=o or {}
    local size=px(pt)
    return ui.Text{id=id,text=s,font_size=size,height=textH(s,size,o.width or CW),shrink=0,
        color=o.color or "onSurface",text_align=o.align,family=o.family,font_weight=o.weight}
end
local function muted(id,s,pt,o)
    o=o or {}
    o.color="onSurfaceVariant"
    return para(id,s,pt,o)
end
local function rule(id,opacity)
    return ui.Box{id=id,width="fill",height=1,shrink=0,fill=ui.alpha("outline",opacity)}
end
-- Rounded text button (QML `ActionButton`): a pill; `selected` fills with the primary role, a nil
-- `on` dims it.
local function pillWidth(label,compact,pt)
    return math.ceil(#label*px(pt or tx.body)*0.55+(compact and sp.xl or sp.huge))
end
local function pill(o)
    local size=px(o.pt or tx.body)
    local lh=math.ceil(size*1.4)
    local h=34
    local armed=o.on~=nil
    local ink=o.selected and "onPrimary" or (armed and "onSurface" or "onSurfaceVariant")
    return kit.Button{id=o.id,width=pillWidth(o.label,o.compact,o.pt),height=h,shrink=0,padding=0,
        padding_top=math.floor((h-lh)/2),radius=rd.full,justify="center",align="center",
        position=o.position,left=o.left,right=o.right,top=o.top,
        fill=o.selected and "primary" or "surfaceContainerHigh",
        hover_fill=o.selected and "primary" or "surfaceContainerHighest",aria_label=o.aria or o.label,on_activate=o.on,
        ui.Text{id=o.id.."-label",text=o.label,height=lh,grow=1,font_size=size,text_align="center",color=ink}}
end
local function logo(id,size)
    return ui.Image{id=id,path="welcome/assets/nacre-logo.themed.svg",width=size,height=size,shrink=0,aria_label=ui.tr("bead.welcome.hero-name")}
end
local function column(id,gap,...)
    return ui.Column{id=id,width="fill",shrink=0,gap=gap,...}
end
local function wrapRow(id,gap,...)
    return ui.Row{id=id,width="fill",height=34,shrink=0,gap=gap,...}
end

-- App tile: glyph, title, detail and an Open / Setup guide / Not installed button.
local function appRow(app)
    local available=get(app.needs)
    local optional=app.optional and not available
    local stacked=CW<640
    local title,detail=ui.tr(app.title),ui.tr(app.detail)
    if optional then detail=ui.tr("bead.welcome.optional-detail",{detail=detail}) end
    local label=optional and ui.tr("bead.welcome.button-setup-guide")
        or (available and ui.tr("bead.welcome.button-open") or ui.tr("bead.welcome.button-not-installed"))
    local action
    if optional then action=ui.action("guide",app.id)
    elseif available then action=act("route:"..app.route) end
    local bw=pillWidth(label)
    local textW=math.max(80,CW-76-(stacked and 18 or bw+40))
    local tsize,dsize=px(13),px(10.5)
    local th,dh=textH(title,tsize,textW),textH(detail,dsize,textW)
    local rowH=math.max(76,16+inkH(title,tsize,textW)+5+inkH(detail,dsize,textW)+(stacked and 34+12+16 or 18))
    return ui.Column{id="app-"..app.id,width="fill",height=rowH,shrink=0,radius=12,fill="surfaceContainer",
        ui.Text{id="glyph",position="absolute",left=18,top=20,width=ic.hero,height=ic.hero,text=app.icon,
            family="Material Symbols Rounded",font_size=px(19),color=optional and ui.alpha("primary",.4) or "primary"},
        ui.Text{id="title",position="absolute",left=76,top=16,width=textW,height=th,text=title,font_size=tsize,
            color=optional and "onSurfaceVariant" or "onSurface"},
        ui.Text{id="detail",position="absolute",left=76,top=16+inkH(title,tsize,textW)+5,width=textW,height=dh,text=detail,font_size=dsize,color="onSurfaceVariant"},
        pill{id="open",label=label,on=action,position="absolute",right=18,top=stacked and 16+inkH(title,tsize,textW)+5+inkH(detail,dsize,textW)+12 or (rowH-34)/2}}
end

local pages={}
function pages.welcome()
    local logoSize=math.max(180,math.min(320,CH*.38))
    local nameSize=math.max(30,math.min(46,CH*.065))
    local tagSize=math.max(13,math.min(17,CW/100))
    return ui.Column{id="hero",width="fill",shrink=0,align="center",gap=sp.xxl,
        ui.Box{id="hero-top",width=1,height=math.max(18,CH*.12),shrink=0},
        logo("hero-logo",logoSize),
        para("hero-name",ui.tr("bead.welcome.hero-name"),nameSize,{align="center",weight=600}),
        muted("hero-tagline",ui.tr("bead.welcome.hero-tagline"),tagSize,{align="center"}),
        muted("hero-tour",ui.tr("bead.welcome.hero-tour"),11,{align="center"})}
end
-- Your colors: bundled scenes and baked palettes, ready before the first frame.
-- The adapter provides cached data; the same kit draws the miniature desktop and role samples.
local function demo(v)
    local labels={brand=ui.tr("bead.colors.card-brand"),files=ui.tr("bead.colors.card-files"),
        lines={ui.tr("bead.colors.card-documents"),ui.tr("bead.colors.card-pictures"),ui.tr("bead.colors.card-projects")},
        open=ui.tr("bead.colors.card-open"),build=ui.tr("bead.colors.card-build"),ready=ui.tr("bead.colors.card-ready"),
        warning=ui.tr("bead.colors.card-warning"),message=ui.tr("bead.colors.card-message"),message_detail=ui.tr("bead.colors.card-message-detail")}
    local cells={}
    if v.available then
        for _,item in ipairs(v.swatches) do
            table.insert(cells,ui.Row{id="role-"..item.role,grow=1,height=34,gap=10,
                ui.Box{id="swatch",width=34,height=34,shrink=0,radius=9,fill=item.color,border_width=1,border_color="outline"},
                ui.Column{id="role-name",grow=1,height=34,justify="center",
                    ui.Text{id="label",text=ui.tr(catalog.roleLabels[item.role]),height=math.ceil(px(10)*1.4),font_size=px(10),color="onSurface"}}})
        end
    else
        for _,item in ipairs(catalog.roles) do
            table.insert(cells,ui.Row{id="role-"..item.role,grow=1,height=34,gap=10,
                ui.Box{id="swatch",width=34,height=34,shrink=0,radius=9,fill=item.role,border_width=1,border_color="outline"},
                ui.Column{id="role-name",grow=1,height=34,justify="center",
                    ui.Text{id="label",text=ui.tr(item.label),height=math.ceil(px(10)*1.4),font_size=px(10),color="onSurface"}}})
        end
    end
    local roles=ui.Row{id="roles",width="fill",height=34,shrink=0,gap=12,table.unpack(cells)}
    if not v.available then
        local note=v.loading and ui.tr("bead.welcome.demo-preparing") or v.error
        return ui.Column{id="demo",width="fill",shrink=0,gap=12,
            muted("demo-state",note~="" and note or ui.tr("bead.welcome.colors-more"),10),roles}
    end
    local tw=(CW-36)/4
    local th=math.max(70,math.min(180,tw*.56,CH*.18))
    local scenes={}
    for _,sc in ipairs(v.scenes) do
        table.insert(scenes,ui.Column{id="scene-"..sc.id,width=tw,shrink=0,gap=7,
            kit.Button{id="pick",width=tw,height=th,shrink=0,padding=4,radius=12,fill="surfaceContainer",hover_fill="surfaceContainerHigh",
                border_width=sc.selected and 2 or 1,border_color=sc.selected and ui.alpha("onSurface",.65) or ui.alpha("outline",.4),
                aria_label=ui.tr("bead.welcome.demo-scene",{name=sc.name}),on_activate=act("demo:choose:"..sc.id),
                ui.Box{id="clip",width="100%",height="100%",radius=9,fill="surfaceContainerHigh",
                    sc.thumb~="" and ui.Image{id="thumb",position="absolute",left=0,top=0,width=tw-8,height=th-8,icon=sc.thumb,aria_label=sc.name} or ui.Box{id="none",width=1,height=1},
                    sc.selected and ui.Box{id="check",position="absolute",left=tw-8-30,top=th-8-30,width=24,height=24,radius=12,fill="surface",
                        ui.Text{id="glyph",position="absolute",left=3,top=3,width=18,height=18,text="check",family="Material Symbols Rounded",font_size=px(10),color="onSurface"}} or ui.Box{id="nocheck",width=1,height=1}}},
            kit.Paragraph{id="name",text=sc.name,width=tw,font_size=px(10),max_lines=2,color="onSurface"}})
    end
    local chips={}
    for i,c in ipairs(v.styles) do chips[i]={id=c.id,label=c.label,selected=c.selected} end
    local modes={}
    for i,m in ipairs{{"dark","dark_mode","bead.welcome.demo-dark"},{"light","light_mode","bead.welcome.demo-light"}} do
        modes[i]=kit.IconButton{id="mode-"..m[1],width=36,height=32,padding=0,radius=rd.small,text=m[2],selected=v.mode==m[1],
            fill=v.mode==m[1] and "primary" or "surfaceContainer",color=v.mode==m[1] and "onPrimary" or "onSurface",
            aria_label=ui.tr(m[3]),on_activate=act("demo:mode:"..m[1])}
    end
    modes.id="modes";modes.gap=sp.xs;modes.shrink=0
    local status=v.error~="" and v.error or (v.busy and ui.tr("bead.welcome.demo-working") or ui.tr("bead.welcome.demo-note"))
    return ui.Column{id="demo",width="fill",shrink=0,gap=10,
        ui.Row{id="scenes",width="fill",shrink=0,gap=12,table.unpack(scenes)},
        ui.Row{id="style-row",width="fill",shrink=0,gap=12,
            ui.Row{id="style-line",grow=1,shrink=1,gap=8,
                ui.Text{id="style-label",text=ui.tr("bead.welcome.demo-style"),width=44,height=32,shrink=0,font_size=px(11),color="onSurface"},
                kit.ChipRow{id="styles",items=chips,per_row=CW>=900 and 8 or 4,radius=rd.full,on_change="welcome.do",change_arg=function(id) return "demo:style:"..id end}},
            ui.Row(modes)},
        muted("style-text",v.style_text,10),
        kit.PreviewCard{id="preview",width=CW,height=math.max(210,math.min(CH-414,280)),colors=v.colors,image=v.has_photo and "colors-card" or "",
            aria_label=ui.tr("bead.colors.preview-aria"),labels=labels},
        roles,
        muted("demo-state",status,10)}
end
function pages.colors()
    return column("colors",12,
        para("colors-title",ui.tr("bead.welcome.colors-title"),25),
        muted("colors-intro",ui.tr("bead.welcome.colors-intro"),12),
        ui.Repeat{id="demo-view",model=ui.bind("welcome.demo"),key="id",render=demo})
end
-- A key as caps: `SUPER+SHIFT+S` -> Super, Shift, S (all-caps names are title-cased, a few are renamed
-- such as RETURN -> Enter; mixed-case keys such as `XF86AudioMute` stay as the compositor wrote them).
local keyNames={SUPER="Super",CTRL="Ctrl",RETURN="Enter",ESCAPE="Esc",PERIOD=".",COMMA=","}
local function caps(key)
    local out={}
    for part in string.gmatch(key,"[^+]+") do
        part=string.match(part,"^%s*(.-)%s*$")
        local up=string.upper(part)
        if part~="" then
            out[#out+1]=keyNames[up] or ((#part>1 and part==up) and string.sub(part,1,1)..string.lower(string.sub(part,2)) or part)
        end
    end
    return out
end
-- Key cap (kit ask: kit.KeyCap): mono caption on the highest container, 1 px outline.
local function keycap(id,label)
    local w=math.ceil(ui.text_width(label,T.fonts.mono,M.text.label))+2*sp.sm
    return ui.Box{id=id,width=math.max(28,w),height=28,shrink=0,radius=6,fill="surfaceContainerHighest",
        border_width=1,border_color=ui.alpha("outline",.6),
        ui.Text{id="label",position="absolute",left=0,top=5,width=math.max(28,w),height=18,text=label,
            family=T.fonts.mono,font_size=M.text.label,text_align="center",color="onSurface"}}
end
local TILE_H=96
local function keyTile(item,width)
    local keys={}
    for i,label in ipairs(caps(item.key)) do keys[i]=keycap("key-"..i,label) end
    keys.id="keys";keys.width="fill";keys.height=28;keys.shrink=0;keys.gap=6
    return ui.Column{id=item.id,width=width,height=TILE_H,shrink=0,radius=M.radius.tile,fill="surfaceContainer",
        padding=sp.lg+2,gap=sp.md,
        ui.Text{id="title",width="fill",height=math.ceil(M.text.body*1.35),shrink=0,text=item.title,font_size=M.text.body,color="onSurface"},
        ui.Row(keys)}
end
-- The rows bound to `ids` (a binding's `Nacre:<id>` action), first bind of each, in `ids` order.
local function byAction(rows,ids)
    local first,out={},{}
    for _,row in ipairs(rows) do
        if row.action and row.action~="" and not first[row.action] then first[row.action]=row end
    end
    for _,id in ipairs(ids) do
        if first[id] then out[#out+1]=first[id] end
    end
    return out
end
function pages.shortcuts()
    local state=get("welcome.shortcuts.state")
    local note=state=="unavailable" and ui.tr("bead.welcome.shortcuts-unavailable")
        or (state=="empty" and ui.tr("bead.welcome.shortcuts-empty") or "")
    local rows=get("welcome.shortcuts")
    local want=catalog.shortcuts
    local grid,panels=byAction(rows,want.grid),byAction(rows,want.panels)
    -- Binds without Nacre titles (custom setups): fill the grid in the topic's order.
    local used={}
    for _,row in ipairs(grid) do used[row.id]=true end
    for _,row in ipairs(panels) do used[row.id]=true end
    for _,row in ipairs(rows) do
        if #grid>=#want.grid then break end
        if not used[row.id] and (row.group or "")=="" then grid[#grid+1]=row;used[row.id]=true end
    end
    local cols=CW>=760 and 4 or 2
    local gap=sp.md
    local tw=math.floor((CW-(cols-1)*gap)/cols)
    local function tiles(id,list)
        local out={}
        for i=1,#list,cols do
            local line={id=id.."-"..math.ceil(i/cols),width="fill",height=TILE_H,shrink=0,gap=gap}
            for j=i,math.min(#list,i+cols-1) do line[#line+1]=keyTile(list[j],tw) end
            out[#out+1]=ui.Row(line)
        end
        return column(id,gap,table.unpack(out))
    end
    local body={para("shortcuts-title",ui.tr("bead.welcome.shortcuts-title"),25),
        muted("shortcuts-intro",ui.tr("bead.welcome.shortcuts-intro"),11)}
    if note~="" then table.insert(body,muted("shortcuts-state",note,10.5)) end
    if #grid>0 then table.insert(body,tiles("shortcut-grid",grid)) end
    if #panels>0 then
        table.insert(body,para("shortcuts-panels",ui.tr("bead.welcome.shortcuts-panels"),13))
        table.insert(body,tiles("shortcut-panels",panels))
    end
    table.insert(body,ui.Row{id="shortcuts-more",width="fill",height=36,shrink=0,gap=sp.lg,align="center",
        kit.Chip{id="shortcuts-all",text=ui.tr("bead.welcome.shortcuts-all"),icon="keyboard",on=false,
            on_activate=act("route:settings:keyboard-mouse")},
        ui.Text{id="shortcuts-note",text=ui.tr("bead.welcome.shortcuts-note"),width=1,grow=1,height=math.ceil(px(10)*1.4),
            font_size=px(10),color="onSurfaceVariant"}})
    return column("shortcuts",14,table.unpack(body))
end
function pages.apps()
    local items={}
    if guide~="" and catalog.guides[guide] then
        local g=catalog.guides[guide]
        local title,detail,setup,status=ui.tr(g.title),ui.tr(g.detail),ui.tr(g.setup),ui.tr(g.status)
        local w=CW-32
        local h=textH(title,px(16),w)+10+textH(detail,px(12),w)+10+textH(setup,px(12),w)+10+textH(status,px(10.5),w)+10+34
        table.insert(items,ui.Column{id="guide",width="fill",height=h+32,shrink=0,radius=12,fill="surfaceContainer",padding=16,gap=10,
            para("guide-title",title,16,{width=w}),para("guide-detail",detail,12,{width=w}),
            muted("guide-setup",setup,12,{width=w}),muted("guide-status",status,10.5,{width=w}),
            ui.Row{id="guide-actions",width="fill",height=34,shrink=0,
                pill{id="guide-back",label=ui.tr("bead.welcome.guide-back"),on=ui.action("guide","")}}})
    end
    for _,app in ipairs(catalog.apps) do table.insert(items,appRow(app)) end
    table.insert(items,muted("apps-note",ui.tr("bead.welcome.apps-note"),10.5))
    return column("apps",14,
        para("apps-title",ui.tr("bead.welcome.apps-title"),23),
        muted("apps-intro",ui.tr("bead.welcome.apps-intro"),12),
        table.unpack(items))
end
function pages.ready()
    return ui.Column{id="ready",width="fill",shrink=0,align="center",gap=18,
        ui.Box{id="ready-top",width=1,height=math.max(0,(CH-340)/2),shrink=0},
        logo("ready-logo",120),
        para("ready-title",ui.tr("bead.welcome.ready-title"),28,{align="center"}),
        muted("ready-note",ui.tr("bead.welcome.ready-note"),10.5,{align="center"}),
        pill{id="ready-help",label=ui.tr("bead.welcome.ready-help"),compact=true,pt=tx.caption,on=go("help")}}
end
function pages.help()
    local rows={para("help-title",ui.tr("bead.welcome.help-title"),23)}
    for index,item in ipairs(catalog.help) do
        table.insert(rows,column("help-"..index,2,
            para("title",ui.tr(item.title),13),muted("detail",ui.tr(item.detail),11),rule("rule",.2)))
    end
    local routes,links={},{}
    for index,item in ipairs(catalog.helpRoutes) do
        table.insert(routes,pill{id="route-"..index,label=ui.tr(item.label),on=act("route:"..item.route)})
    end
    for index,item in ipairs(catalog.links) do
        table.insert(links,pill{id="link-"..index,label=ui.tr(item.label),on=act("link:"..item.link)})
    end
    table.insert(rows,wrapRow("help-routes",8,table.unpack(routes)))
    table.insert(rows,para("help-online",ui.tr("bead.welcome.help-online"),15))
    table.insert(rows,wrapRow("help-links",8,table.unpack(links)))
    table.insert(rows,muted("help-about",ui.tr("bead.welcome.help-about"),10.5))
    return column("help",14,table.unpack(rows))
end

-- Back / Skip tour, the step rail, Next / Start using Nacre.
local steps=catalog.steps
local index=0
for i,name in ipairs(steps) do if name==page then index=i end end
local backLabel=(index==1 and page~="help") and ui.tr("bead.welcome.skip-tour") or ui.tr("bead.welcome.back")
local backAction
if page=="help" then backAction=go("ready")
elseif index>1 then backAction=go(steps[index-1])
else backAction=act("close") end
local nextLabel,nextAction=ui.tr("bead.welcome.next"),nil
if page=="ready" then nextLabel,nextAction=ui.tr("bead.welcome.start"),act("close")
elseif page=="help" then nextLabel,nextAction=ui.tr("bead.welcome.back-to-finish"),go("ready")
else nextAction=go(steps[index+1] or "ready") end
-- Footer (design review 13): quiet Skip tour / Back, a rail of named step chips, primary Next with an arrow.
local labelH=math.ceil(M.text.label*1.35)
local function quiet(id,label,action)
    -- text_width measures weight 400; the 500/600 labels paint a few percent wider.
    local w=math.ceil(ui.text_width(label,ui.default_font_family,M.text.label)*1.08)
    local width=w+2*sp.lg
    return kit.Button{id=id,width=width,height=34,padding=0,radius=rd.full,shrink=0,position="absolute",left=0,top=15,
        fill="transparent",hover_fill=ui.alpha("onSurface",.08),aria_label=label,on_activate=action,
        ui.Row{id="content",width=width,height=34,align="center",justify="center",
            ui.Text{id="label",text=label,width=w,height=labelH,font_size=M.text.label,font_weight=500,text_align="center",color="onSurfaceVariant"}}},width
end
local function primary(id,label,action)
    local w=math.ceil(ui.text_width(label,ui.default_font_family,M.text.label)*1.08)
    local glyph=ic.small*4/3
    local width=w+6+glyph+2*sp.lg+2
    return kit.Button{id=id,width=width,height=34,padding=0,radius=rd.full,shrink=0,position="absolute",right=0,top=15,
        fill="primary",hover_fill=ui.tint("primary","onPrimary",.1),aria_label=label,on_activate=action,
        ui.Row{id="content",width=width,height=34,align="center",justify="center",gap=6,
            ui.Text{id="label",text=label,width=w,height=labelH,font_size=M.text.label,font_weight=600,text_align="center",color="onPrimary"},
            ui.Text{id="icon",text="arrow_forward",family=T.fonts.icon,font_size=glyph,width=glyph+2,height=24,color="onPrimary"}}},width
end
local back,backW=quiet("back",backLabel,backAction)
local nextButton,nextW=primary("next",nextLabel,nextAction)
-- Chips name the steps; the current one fades to primaryContainer (dashboard dot timing). In a window
-- too narrow for every name, the other steps show their number.
local railW=CW-backW-nextW-2*sp.md
local function chipW(label) return math.ceil(ui.text_width(label,ui.default_font_family,M.text.label))+2*sp.lg+sp.hairline end
local named=-sp.sm
for _,name in ipairs(steps) do named+=chipW(ui.tr(catalog.chips[name]))+sp.sm end
local chips={id="rail",position="absolute",left=backW+sp.md,top=14,width=railW,height=36,gap=sp.sm,justify="center"}
for i,name in ipairs(steps) do
    local on=name==page
    table.insert(chips,kit.Chip{id="step-"..name,text=(on or named<=railW) and ui.tr(catalog.chips[name]) or tostring(i),on=on,
        fill=ui.mix("surfaceContainerHigh","primaryContainer",ui.animated("welcome.rail."..name,on and 1 or 0,motion.duration.switch,motion.curve.out_cubic)),
        aria_label=ui.tr("bead.welcome.step",{number=i,name=ui.tr(catalog.sections[name])}),on_activate=go(name)})
end
-- Child order is the Tab order: Skip / Back, the steps, Next.
local footer=ui.Column{id="footer",width="fill",height=64,shrink=0,
    rule("footer-rule",.3),
    back,
    ui.Row(chips),
    nextButton}

local function section()
    return catalog.sections[page] and ui.tr(catalog.sections[page]) or ""
end
local header=ui.Column{id="header",width="fill",height=64,shrink=0,
    ui.Row{id="header-row",width="fill",height=63,align="center",padding_left=sp.xl,padding_right=sp.xl,gap=sp.lg,
        ui.Image{id="header-logo",path="welcome/assets/nacre-logo.themed.svg",width=ic.hero,height=ic.hero,shrink=0},
        ui.Text{id="header-title",text=ui.tr("bead.welcome.title"),height=math.ceil(px(tx.title)*1.4),font_size=px(tx.title),width=1,grow=1,color="onSurface"},
        ui.Text{id="header-section",text=section(),width=240,height=math.ceil(px(tx.caption)*1.4),font_size=px(tx.caption),
            text_align="right",color="onSurfaceVariant"}},
    rule("header-rule",.35)}

-- A page enters 24 px from the side it comes from (help counts as after Ready); the first page
-- of a window does not move. Paint-only: the marker shifts the cached page, no Luau per frame.
local function position(name) for i,n in ipairs(steps) do if n==name then return i end end return #steps+1 end
local travel=(from=="" or from==page) and 0 or (position(page)>position(from) and 1 or -1)
return ui.Column{id="welcome",width="100%",height="100%",fill="surface",
    -- The wallpaper tint (design review 13): a primaryContainer glow from the top-left corner.
    ui.Image{id="tint",path="welcome/assets/hero-tint.themed.svg",position="absolute",left=0,top=0,width=W,height=H,image_fit="fill"},
    header,
    ui.Column{id="body",width="fill",grow=1,padding_left=sp.xxl,padding_right=sp.xxl,padding_top=sp.md+sp.xs,padding_bottom=sp.md+sp.xs,
        ui.Scroll{id="canvas",scroll=ui.bind("welcome.scroll"),grow=1,
            ui.Column{id="page",width="fill",shrink=0,
                translate_x=ui.animated("welcome.page."..page,0,motion.duration.peek,motion.curve.out_cubic,travel*24),
                (pages[page] or pages.welcome)()}},
        ui.Box{id="gap",width=1,height=sp.md+sp.xs,shrink=0},
        footer}}
