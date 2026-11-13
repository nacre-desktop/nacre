-- SPDX-License-Identifier: GPL-3.0-only
-- Brain app (N0217, phase 1): a side rail (screens, worlds, index status), a toolbar (search,
-- Ask, Capture, New) and plain pages: Overview, Search with the Ask card, the note reader with
-- its backlinks, and Loops. All data is the `brain.rs` projection of the worker's replies; sizes
-- and colours come from the kit tokens and palette roles. The Graph screen waits for
-- kit.GraphCanvas (N0236) and stays hidden until `brain.graph_available`.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local sp,rd,tx=T.spacing,T.radii,T.text
local function get(n) return ui.resolve_for_check(ui.bind(n)) end
local function px(pt) return pt*4/3 end
local function act(arg) return ui.action("brain.do",arg) end
-- Palette roles by world order (the legend and every world dot/chip).
local WORLD_ROLES={"blue","mauve","green","peach","pink","teal","yellow","red","lavender","sapphire","maroon","sky"}
local function role(index)
    local i=tonumber(index) or 0
    if i<1 then return "outline" end
    return WORLD_ROLES[(i-1)%#WORLD_ROLES+1]
end
local W=get("brain.win_w")
local SCREEN=get("brain.screen")
local STATUS=get("brain.status")
local RAIL=232
local TOP=56
local CW=W-RAIL
local wide=W>=1400

local function text(id,s,size,color,w,h,extra)
    local p={id=id,text=s,width=w or "fill",height=h or math.ceil(px(size)*1.4),shrink=0,font_size=px(size),color=color or "onSurface"}
    if extra then for k,v in pairs(extra) do p[k]=v end end
    return ui.Text(p)
end
local function para(id,s,size,w,lines,color) return kit.Paragraph{id=id,text=s,width=w,font_size=px(size),max_lines=lines,color=color or "onSurface"} end
local function dot(id,color,d) return ui.Box{id=id,width=d,height=d,radius=d/2,fill=color,shrink=0} end
local function chip_w(s,color) return (utf8.len(s) or 4)*px(tx.caption)*.58+28+(color and 14 or 0) end
local function chip(id,s,color,fill,action)
    local w=chip_w(s,color)
    local row={id="r",height=26,align="center",gap=6}
    if color then table.insert(row,dot("dot",color,8)) end
    table.insert(row,text("t",s,tx.caption,"onSurface",w-18-(color and 14 or 0),18))
    return kit.Button{id=id,height=26,width=w,shrink=0,radius=13,padding=0,padding_left=9,padding_right=9,fill=fill or "surfaceContainerHigh",
        hover_fill=action and "surfaceContainerHighest" or (fill or "surfaceContainerHigh"),aria_label=s,on_activate=action,ui.Row(row)}
end
local function btn(id,s,primary,w,action)
    w=w or (utf8.len(s) or 4)*8+28
    return kit.Button{id=id,text=s,width=w,height=34,shrink=0,padding=0,padding_top=(34-px(tx.body)*1.3)/2,radius=rd.full,
        fill=primary and "primary" or "surfaceContainerHigh",color=primary and "onPrimary" or "onSurface",font_size=px(tx.body),text_align="center",aria_label=s,on_activate=action}
end
local function card(id,w,h,kids,fill)
    kids=kids or {}
    kids.id=id;kids.width=w;kids.height=h;kids.shrink=0;kids.padding=sp.lg;kids.gap=sp.sm;kids.radius=rd.medium;kids.fill=fill or "surfaceContainer"
    return kit.Card(kids)
end
local function heading(id,s,w) return text(id,s,tx.title,"onSurface",w,22,{font_weight=600}) end
local function caption(id,s,w) return text(id,s,tx.caption,"onSurfaceVariant",w,16,{font_weight=600}) end
local function vdiv(id) return ui.Box{id=id,width="fill",height=1,shrink=0,fill=ui.alpha("outline",.25)} end
local function open_arg(n) return "open:"..tostring(n.id).."|"..tostring(n.path or "") end
-- A note row: world dot, label, trailing caption; activates the reader.
local function note_row(id,n,w,trailing)
    return kit.Button{id=id,height=30,width=w,shrink=0,padding=0,padding_left=sp.xs,padding_right=sp.xs,radius=rd.small,fill="transparent",aria_label=n.label,on_activate=act(open_arg(n)),
        ui.Row{id="r",height=30,width=w-2*sp.xs,gap=sp.sm,align="center",dot("d",role(n.world_index),8),
            para("t",n.label,tx.body,w-2*sp.xs-8-2*sp.sm-100,1),text("s",trailing or "",tx.caption,"onSurfaceVariant",100,16,{text_align="right"})}}
end
local function empty(id,s,w) return text(id,s,tx.body,"onSurfaceVariant",w,20) end

-- ---------- status ----------
local function status_line()
    local counts=get("brain.counts")[1]
    if STATUS=="error" then return get("brain.error") end
    if STATUS=="connecting" then return ui.tr("bead.brain.connecting") end
    if not counts or counts.notes==0 then return ui.tr("bead.brain.indexing") end
    return ui.tr("bead.brain.notes-indexed",{count=tostring(counts.notes)})
end

-- ---------- rail + toolbar ----------
local function rail()
    local kids={ui.Row{id="brand",height=36,width=RAIL-2*sp.lg,gap=sp.sm,align="center",shrink=0,
        ui.Box{id="logo",width=28,height=28,radius=14,fill="primary",shrink=0,
            ui.Text{id="g",text="psychology",family=T.fonts.icon,width=20,height=20,font_size=px(T.icons.medium),color="onPrimary",text_align="center",position="absolute",left=4,top=4}},
        text("n",ui.tr("bead.brain.title"),tx.title,"onSurface",RAIL-36-2*sp.lg,24,{font_weight=600})},
        ui.Box{id="gap0",height=sp.md,width=1}}
    local nav={{"overview",ui.tr("bead.brain.overview")}}
    -- Until kit.GraphCanvas lands (N0236) the entry opens the web Brain's graph (spec phase 1).
    if get("brain.graph_available") then table.insert(nav,{"graph",ui.tr("bead.brain.graph")}) else table.insert(nav,{"web",ui.tr("bead.brain.graph-web")}) end
    if get("brain.note")~="" or get("brain.note_path")~="" then table.insert(nav,{"note",ui.tr("bead.brain.reader")}) end
    table.insert(nav,{"search",ui.tr("bead.brain.search")})
    table.insert(nav,{"loops",ui.tr("bead.brain.loops")})
    for _,n in ipairs(nav) do
        local active=SCREEN==n[1]
        table.insert(kids,kit.Button{id="nav-"..n[1],height=34,width=RAIL-2*sp.lg,shrink=0,radius=rd.small,padding=0,padding_left=sp.md,padding_top=(34-px(tx.body)*1.3)/2,
            fill=active and "primaryContainer" or "transparent",hover_fill=active and "primaryContainer" or "surfaceContainerHigh",
            text=n[2],color=active and "onPrimaryContainer" or "onSurface",font_size=px(tx.body),aria_label=n[2],on_activate=act(n[1]=="web" and "web" or "screen:"..n[1])})
    end
    table.insert(kids,ui.Box{id="gap1",height=sp.lg,width=1})
    table.insert(kids,caption("wl",ui.tr("bead.brain.worlds"),RAIL-2*sp.lg))
    local worlds={}
    for i,wd in ipairs(get("brain.worlds")) do
        worlds[i]=kit.Button{id="w"..i,height=28,width=RAIL-2*sp.lg,shrink=0,radius=rd.small,padding=0,padding_left=sp.md,padding_right=sp.sm,fill=wd.selected and "secondaryContainer" or "transparent",
            hover_fill="surfaceContainerHigh",aria_label=wd.label,on_activate=act("world:"..wd.label),
            ui.Row{id="r",height=28,width=RAIL-2*sp.lg-sp.md-sp.sm,gap=sp.sm,align="center",dot("d",role(wd.index),8),
                para("t",wd.label,tx.body,RAIL-2*sp.lg-sp.md-sp.sm-8-2*sp.sm-40,1),text("c",tostring(wd.count),tx.caption,"onSurfaceVariant",40,16,{text_align="right"})}}
    end
    worlds.id="worlds";worlds.scroll=ui.bind("brain.rail.scroll");worlds.width=RAIL-2*sp.lg;worlds.grow=1
    table.insert(kids,ui.Scroll(worlds))
    table.insert(kids,text("st",status_line(),tx.caption,"onSurfaceVariant",RAIL-2*sp.lg,16))
    table.insert(kids,text("pv",ui.tr("bead.brain.private"),tx.caption-1,"onSurfaceVariant",RAIL-2*sp.lg,14))
    kids.id="rail";kids.width=RAIL;kids.height="100%";kids.shrink=0;kids.padding=sp.lg;kids.gap=2;kids.fill="surfaceContainerLow"
    return ui.Column(kids)
end
local function toolbar()
    local fw=CW-2*sp.xxl-3*sp.sm-62-82-60
    return ui.Row{id="toolbar",height=TOP,width=CW,shrink=0,align="center",gap=sp.sm,padding_left=sp.xxl,padding_right=sp.xxl,
        kit.TextInput{id="search",text=ui.bind("brain.query"),placeholder=ui.tr("bead.brain.search-placeholder"),aria_label=ui.tr("bead.brain.search-placeholder"),
            width=fw,height=36,shrink=0,radius=rd.full,fill="surfaceContainerHigh",padding_left=sp.lg,padding_right=sp.lg,font_size=px(tx.body),on_activate=act("screen:search")},
        btn("ask",ui.tr("bead.brain.ask"),false,62,act("screen:search")),
        btn("capture",ui.tr("bead.brain.capture"),false,82,act("action:capture")),
        btn("new",ui.tr("bead.brain.new"),true,60,act("action:new"))}
end

-- ---------- overview ----------
local function stat_tiles(w)
    local c=get("brain.counts")[1] or {notes=0,worlds=0,review=0,loops=0}
    local items={{ui.tr("bead.brain.stat-notes"),c.notes},{ui.tr("bead.brain.stat-worlds"),c.worlds},
        {ui.tr("bead.brain.stat-review"),c.review},{ui.tr("bead.brain.stat-loops"),c.loops}}
    local kids={}
    local tw=(w-(#items-1)*sp.md)/#items
    for i,it in ipairs(items) do
        kids[i]=card("s"..i,tw,78,{text("v",tostring(it[2]),22,"onSurface",tw-2*sp.lg,32),text("l",it[1],tx.caption,"onSurfaceVariant",tw-2*sp.lg,16)})
    end
    kids.id="stats";kids.width=w;kids.height=78;kids.gap=sp.md;kids.shrink=0
    return ui.Row(kids)
end
local function world_cards(w,cols,hgt)
    local worlds=get("brain.worlds")
    local cw=(w-(cols-1)*sp.md)/cols
    local most=1
    for _,wd in ipairs(worlds) do most=math.max(most,wd.count or 0) end
    local rows={}
    for i,wd in ipairs(worlds) do
        local ar={}
        local used=0
        for j,a in ipairs(wd.areas or {}) do
            local cw2=(utf8.len(a.label) or 4)*px(tx.caption)*.58+22
            if used+cw2>cw-2*sp.lg then break end
            used+=cw2+6
            ar[j]=chip("a"..j,a.label,nil,ui.alpha("onSurface",.07))
        end
        ar.id="areas";ar.gap=6;ar.height=26;ar.width=cw-2*sp.lg;ar.shrink=0
        local c=kit.Button{id="w"..i,width=cw,height=hgt,shrink=0,padding=sp.lg,radius=rd.medium,fill="surfaceContainer",hover_fill="surfaceContainerHigh",aria_label=wd.label,on_activate=act("world:"..wd.label),
            ui.Column{id="c",width=cw-2*sp.lg,gap=sp.sm,
                ui.Row{id="h",height=22,width=cw-2*sp.lg,gap=sp.sm,align="center",dot("d",role(wd.index),10),text("n",wd.label,tx.title,"onSurface",cw-2*sp.lg-70,22),text("c",tostring(wd.count),tx.body,"onSurfaceVariant",50,20,{text_align="right"})},
                ui.Box{id="bar",width=math.max(4,(cw-2*sp.lg)*(wd.count or 0)/most),height=4,radius=2,fill=ui.alpha(role(wd.index),.7),shrink=0},
                ui.Row(ar)}}
        local r=math.ceil(i/cols)
        rows[r]=rows[r] or {id="r"..r,gap=sp.md,height=hgt,shrink=0}
        table.insert(rows[r],c)
    end
    local kids={}
    for i,r in ipairs(rows) do kids[i]=ui.Row(r) end
    kids.id="worlds";kids.width=w;kids.gap=sp.md;kids.shrink=0
    return ui.Column(kids)
end
local function overview()
    local iw=CW-2*sp.xxl
    local recent,loops={},{}
    for i,n in ipairs(get("brain.recent")) do if i<=5 then recent[i]=note_row("r"..i,n,(iw-sp.md)*.58-2*sp.lg,n.date) end end
    for i,l in ipairs(get("brain.loops")) do if i<=5 then loops[i]=note_row("l"..i,{id=l.source,path=l.path,label=l.text,world_index=l.world_index},(iw-sp.md)*.42-2*sp.lg,l.date) end end
    if #recent==0 then recent[1]=empty("none",ui.tr("bead.brain.no-notes"),(iw-sp.md)*.58-2*sp.lg) end
    if #loops==0 then loops[1]=empty("none",ui.tr("bead.brain.no-loops"),(iw-sp.md)*.42-2*sp.lg) end
    recent.id="rl";recent.gap=2;recent.shrink=0;loops.id="ll";loops.gap=2;loops.shrink=0
    return ui.Scroll{id="overview",width=CW,grow=1,scroll=ui.bind("brain.scroll"),
        ui.Column{id="page",width=CW,padding=sp.xxl,gap=sp.lg,shrink=0,
            text("h1",ui.tr("bead.brain.your-brain"),22,"onSurface",iw,40,{font_weight=600}),
            text("sub",ui.tr("bead.brain.tagline"),tx.body,"onSurfaceVariant",iw,20),
            stat_tiles(iw),
            heading("wh",ui.tr("bead.brain.worlds"),iw),
            world_cards(iw,4,100),
            ui.Row{id="bottom",width=iw,gap=sp.md,shrink=0,
                card("recent",(iw-sp.md)*.58,210,{heading("t",ui.tr("bead.brain.recent")),ui.Column(recent)}),
                card("loops",(iw-sp.md)*.42,210,{heading("t",ui.tr("bead.brain.loops")),ui.Column(loops)})}}}
end

-- ---------- search + ask ----------
local function results(w)
    local kids={}
    local query=get("brain.query")
    for i,r in ipairs(get("brain.results")) do
        local world=r.world~="" and r.world or ui.tr("bead.brain.unfiled")
        local tw=w-2*sp.sm-chip_w(world,true)-84-2*sp.sm
        kids[i]=kit.Button{id="r"..i,width=w,height=74,shrink=0,padding=sp.sm,radius=rd.small,fill="transparent",hover_fill="surfaceContainerHigh",aria_label=r.label,on_activate=act(open_arg(r)),
            ui.Column{id="c",width=w-2*sp.sm,gap=3,
                ui.Row{id="h",height=22,width=w-2*sp.sm,gap=sp.sm,align="center",text("t",r.label,tx.body+1,"onSurface",tw,22,{font_weight=600}),
                    chip("w",world,role(r.world_index)),text("s",r.date,tx.caption,"onSurfaceVariant",84,16,{text_align="right"})},
                para("sn",r.snippet or "",tx.caption,w-2*sp.sm,2,"onSurfaceVariant")}}
    end
    if #kids==0 then
        local msg=query=="" and get("brain.world")=="" and ui.tr("bead.brain.search-hint") or ui.tr("bead.brain.no-results")
        kids[1]=empty("none",msg,w)
    end
    kids.id="results";kids.width=w;kids.gap=2;kids.shrink=0
    return ui.Column(kids)
end
local function ask_card(w,h)
    local enabled=get("brain.ask.enabled")
    local answer,err=get("brain.ask.answer"),get("brain.ask.error")
    local kids={ui.Row{id="head",height=32,width=w-2*sp.lg,align="center",gap=sp.sm,heading("t",ui.tr("bead.brain.ask-title"),w-2*sp.lg-46-sp.sm),
        kit.Switch{id="enable",checked=enabled,aria_label=ui.tr("bead.brain.ask-enable"),on_activate=act("ask.toggle")}},
        text("d",enabled and ui.tr("bead.brain.ask-enabled-note") or ui.tr("bead.brain.ask-disabled-note"),tx.caption-1,"onSurfaceVariant",w-2*sp.lg,28)}
    if enabled then
        table.insert(kids,kit.TextInput{id="question",text=ui.bind("brain.ask.text"),placeholder=ui.tr("bead.brain.ask-placeholder"),aria_label=ui.tr("bead.brain.ask-placeholder"),
            width=w-2*sp.lg,height=36,shrink=0,radius=rd.small,fill="surfaceContainerHigh",font_size=px(tx.body),on_activate=act("ask")})
        table.insert(kids,btn("send",get("brain.ask.busy") and ui.tr("bead.brain.asking") or ui.tr("bead.brain.ask"),true,nil,not get("brain.ask.busy") and act("ask") or nil))
    end
    if err~="" then table.insert(kids,para("err",err,tx.body,w-2*sp.lg,4,"error")) end
    if answer~="" then
        table.insert(kids,para("a",answer,tx.body+1,w-2*sp.lg,10))
        local src={caption("st",ui.tr("bead.brain.sources"))}
        for i,c in ipairs(get("brain.ask.citations")) do table.insert(src,text("s"..i,i..".  "..tostring(c.text),tx.body,"primary",nil,20)) end
        src.id="src";src.width=w-2*sp.lg;src.gap=2;src.shrink=0
        table.insert(kids,ui.Column(src))
    end
    return card("ask",w,h,kids,"surfaceContainerLow")
end
local function search()
    local side=400
    local rw=CW-side-3*sp.xxl
    local count=get("brain.result_count")
    local query=get("brain.query")
    local title=query=="" and ui.tr("bead.brain.search") or ui.tr("bead.brain.results-for",{count=tostring(count),query=query})
    local filters,names={{id="all",label=ui.tr("bead.brain.all-worlds"),selected=get("brain.world")==""}},{}
    for i,wd in ipairs(get("brain.worlds")) do names["w"..i]=wd.label;table.insert(filters,{id="w"..i,label=wd.label,selected=wd.selected}) end
    local dym=get("brain.did_you_mean")
    local main={id="main",width=rw,height="100%",gap=sp.md,clip_children=true,heading("h1",title,rw)}
    if dym~="" then table.insert(main,text("dym",ui.tr("bead.brain.did-you-mean",{query=dym}),tx.caption,"onSurfaceVariant",rw,16)) end
    table.insert(main,kit.ChipRow{id="filters",items=filters,per_row=6,on_change="brain.do",change_arg=function(id) return "world:"..(names[id] or "") end})
    table.insert(main,ui.Scroll{id="list",width=rw,grow=1,scroll=ui.bind("brain.scroll"),results(rw)})
    return ui.Row{id="search",width=CW,grow=1,clip_children=true,padding=sp.xxl,padding_top=sp.xl,gap=sp.xxl,ui.Column(main),ask_card(side,nil)}
end

-- ---------- note reader ----------
local function links(id,title,items,w)
    local kids={caption("t",title.." ("..#items..")",w)}
    for i,it in ipairs(items) do table.insert(kids,note_row("r"..i,it,w,it.date)) end
    if #items==0 then table.insert(kids,empty("none",ui.tr("bead.brain.no-links"),w)) end
    kids.id=id;kids.width=w;kids.gap=2;kids.shrink=0
    return ui.Column(kids)
end
local function reader(p,w)
    local crumb={id="crumb",height=26,gap=6,align="center",btn("back",ui.tr("bead.brain.back"),false,nil,act("back"))}
    if p.world~="" then table.insert(crumb,chip("c1",p.world,role(p.world_index))) end
    if p.area and p.area~="" then table.insert(crumb,chip("c2",p.area)) end
    local kids={ui.Row(crumb),text("h1",p.title,24,"onSurface",w,46,{font_weight=600})}
    if p.state=="loading" then
        table.insert(kids,empty("loading",ui.tr("bead.brain.loading"),w))
    else
        if p.path~="" then table.insert(kids,ui.Row{id="actions",height=34,gap=sp.sm,btn("open",ui.tr("bead.brain.open-editor"),false,nil,act("action:open-note"))}) end
        if #p.facts>0 then
            local facts={caption("ft",ui.tr("bead.brain.facts"))}
            for i,f in ipairs(p.facts) do
                table.insert(facts,para("f"..i,"•  "..f.text..(f.superseded and " ("..ui.tr("bead.brain.superseded")..")" or ""),tx.body,w-2*sp.lg,4,(f.stale or f.superseded) and "onSurfaceVariant" or "onSurface"))
            end
            table.insert(kids,card("facts",w,nil,facts,"surfaceContainerHigh"))
        end
        for i,b in ipairs(p.blocks) do
            if b.kind=="heading" then table.insert(kids,heading("b"..i,b.text,w))
            elseif b.kind=="bullet" then table.insert(kids,para("b"..i,"•  "..b.text,tx.body+1,w,8))
            else table.insert(kids,para("b"..i,b.text,tx.body+1,w,40)) end
        end
        if #p.blocks==0 and #p.facts==0 then table.insert(kids,empty("empty",ui.tr("bead.brain.empty-note"),w)) end
    end
    kids.id="reader";kids.width=w;kids.gap=sp.md;kids.shrink=0
    return ui.Column(kids)
end
local function note()
    local p=get("brain.page")[1] or {title="",world="",world_index=0,facts={},blocks={},backlinks={},links_out={},state="loading",path=""}
    local side=wide and 400 or 340
    local rw=math.min(720,CW-side-3*sp.xxl)
    return ui.Row{id="note",width=CW,grow=1,clip_children=true,
        ui.Scroll{id="main",grow=1,height="100%",scroll=ui.bind("brain.scroll"),
            ui.Column{id="body",width=rw+2*sp.xxl,padding=sp.xxl,padding_top=sp.xl,shrink=0,reader(p,rw)}},
        ui.Column{id="side",width=side,height="100%",shrink=0,padding=sp.lg,gap=sp.md,fill="surfaceContainerLow",clip_children=true,
            links("back",ui.tr("bead.brain.backlinks"),p.backlinks,side-2*sp.lg),
            links("out",ui.tr("bead.brain.links-out"),p.links_out,side-2*sp.lg)}}
end

-- ---------- loops ----------
local function loops()
    local iw=CW-2*sp.xxl
    local kids={}
    for i,l in ipairs(get("brain.loops")) do
        kids[i]=kit.Button{id="l"..i,width=iw,height=56,shrink=0,padding=sp.sm,radius=rd.small,fill="transparent",hover_fill="surfaceContainerHigh",aria_label=l.text,on_activate=act("open:"..tostring(l.source).."|"..tostring(l.path or "")),
            ui.Row{id="r",width=iw-2*sp.sm,height=40,gap=sp.md,align="center",dot("d",role(l.world_index),8),
                para("t",l.text,tx.body,iw-2*sp.sm-8-sp.md-120-sp.md,2),
                ui.Column{id="m",width=120,gap=2,shrink=0,text("w",l.world or "",tx.caption,"onSurfaceVariant",120,16,{text_align="right"}),text("d",l.date or "",tx.caption,"onSurfaceVariant",120,16,{text_align="right"})}}}
    end
    if #kids==0 then kids[1]=empty("none",ui.tr("bead.brain.no-loops"),iw) end
    kids.id="rows";kids.width=iw;kids.gap=2;kids.shrink=0
    return ui.Scroll{id="loops",width=CW,grow=1,scroll=ui.bind("brain.scroll"),
        ui.Column{id="page",width=CW,padding=sp.xxl,gap=sp.lg,shrink=0,
            text("h1",ui.tr("bead.brain.open-loops"),22,"onSurface",iw,40,{font_weight=600}),
            text("sub",ui.tr("bead.brain.loops-tagline"),tx.body,"onSurfaceVariant",iw,20),
            ui.Column(kids)}}
end

local function content()
    if STATUS=="error" then
        return ui.Column{id="error",width=CW,grow=1,padding=sp.xxl,gap=sp.md,
            heading("h1",ui.tr("bead.brain.unavailable"),CW-2*sp.xxl),para("e",get("brain.error"),tx.body,CW-2*sp.xxl,4,"onSurfaceVariant"),
            ui.Row{id="r",height=34,btn("retry",ui.tr("bead.brain.retry"),true,nil,act("refresh"))}}
    end
    if SCREEN=="note" then return note() end
    if SCREEN=="search" then return search() end
    if SCREEN=="loops" then return loops() end
    return overview()
end

return ui.Row{id="brain",width="100%",height="100%",fill="surface",rail(),
    ui.Column{id="main",grow=1,height="100%",clip_children=true,toolbar(),vdiv("rule"),content()}}
