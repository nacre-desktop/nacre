-- SPDX-License-Identifier: GPL-3.0-only
-- Nacre AI app (N0248, design review D8 option A): the AI panel's look in a window with a rail
-- (Chats, Projects, Accounts, Usage, Brain; Settings at the foot). Chats lists `ai.chats.list`
-- rows in the D7 sidebar row style (D8 hook: host the sidebar's launcher view itself once
-- shell/chat exposes it as a function module); Projects and Accounts are settings-style rows;
-- Usage is one ring per reported quota window; Brain opens the Brain app. Every provider name
-- and id comes from the `ai.providers` store, sizes from the kit tokens and metrics.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local sp,M=T.spacing,kit.metrics
local function get(n) return ui.resolve_for_check(ui.bind(n)) end
local function act(arg) return ui.action("ai.do",arg) end
local function line(size) return math.ceil(size*1.35) end
local W=get("ai.win_w")
local PAGE=get("ai.page")
local BODY,CAPTION,TITLE=M.text.body,M.text.caption,M.text.title
local ICON=T.icons.large
local RAIL=sp.huge*6+sp.sm
local ITEM=sp.huge+sp.xs
local BTN=sp.huge+sp.hairline
local PAD_X,PAD_Y=sp.xl,sp.md+sp.xs
local CW=W-RAIL
local IW=CW-2*PAD_X
local providers,projects=get("ai.providers"),get("ai.projects")
-- The view's choice is the row the adapter marked `selected`.
local function chosen(rows)
    for _,r in ipairs(rows) do if r.selected then return r end end
end
local provider,project=chosen(providers),chosen(projects)
local function glyph(id,name,size,color,w,h)
    return ui.Text{id=id,text=name,family=T.fonts.icon,width=w or size+2,height=h or line(size),font_size=size,text_align="center",color=color,shrink=0}
end
local function label(id,s,size,color,w,extra)
    local p={id=id,text=s,width=w,height=line(size),font_size=size,color=color or "onSurface",shrink=0}
    if extra then for k,v in pairs(extra) do p[k]=v end end
    return ui.Text(p)
end
local function icon_button(id,name,aria,action)
    return kit.IconButton{id=id,text=name,aria_label=aria,width=BTN,height=BTN,padding=0,radius=T.radii.full,font_size=ICON,shrink=0,
        fill="transparent",hover_fill=ui.alpha("onSurface",.08),color=action and "onSurface" or "onSurfaceVariant",focus_ring=true,on_activate=action}
end

-- ---------- rail ----------
local RAIL_W=RAIL-2*sp.compact
local function rail_item(id,name,text,on,action)
    return kit.Button{id="nav-"..id,width=RAIL_W,height=ITEM,shrink=0,padding=0,padding_left=sp.compact,radius=sp.compact,
        fill=on and "surfaceContainerHigh" or "transparent",hover_fill=on and "surfaceContainerHigh" or ui.alpha("onSurface",.06),
        focus_ring=true,aria_label=text,on_activate=action,
        ui.Row{id="row",width=RAIL_W-sp.compact,height=ITEM,gap=sp.compact,align="center",
            glyph("icon",name,ICON,on and "primary" or "onSurfaceVariant"),
            label("label",text,BODY,"onSurface",RAIL_W-3*sp.compact-ICON-2,{font_weight=on and 500 or 400})}}
end
local function rail()
    local kids={id="rail",width=RAIL,height="100%",shrink=0,padding_left=sp.compact,padding_right=sp.compact,padding_top=sp.lg,padding_bottom=sp.lg,
        gap=sp.hairline,fill="surfaceContainer",
        ui.Row{id="brand",width=RAIL_W,height=T.icons.xlarge+sp.lg+sp.hairline,gap=sp.sm,align="start",padding_left=sp.sm,padding_top=sp.xs,shrink=0,
            ui.Image{id="logo",width=T.icons.xlarge,height=T.icons.xlarge,path="chat/assets/nacre-ai.themed.svg",aria_label=ui.tr("bead.panel.chat-title")},
            label("name",ui.tr("bead.panel.chat-title"),TITLE+1,"onSurface",RAIL_W-sp.sm-T.icons.xlarge-sp.sm,{font_weight=600})}}
    for _,n in ipairs({{"chats","forum","bead.panel.ai-chats"},{"projects","folder","bead.panel.ai-projects"},
        {"accounts","person","bead.panel.ai-accounts"},{"usage","data_usage","bead.panel.ai-usage"}}) do
        table.insert(kids,rail_item(n[1],n[2],ui.tr(n[3]),PAGE==n[1],ui.action("ai.page",n[1])))
    end
    table.insert(kids,rail_item("brain","hub",ui.tr("bead.panel.ai-brain"),false,act("brain")))
    table.insert(kids,ui.Box{id="spacer",width=1,height=0,grow=1})
    table.insert(kids,rail_item("settings","settings",ui.tr("bead.panel.chat-tab-settings"),false,get("ai.connected") and act("settings") or nil))
    return ui.Column(kids)
end

-- ---------- shared page pieces ----------
-- The page title and its actions; `right` may hold nils (absent actions).
local function header(title,right)
    local row={id="header",width=IW,height=BTN,gap=sp.sm,align="center",shrink=0,
        label("title",title,TITLE,"onSurface",nil,{width=0,grow=1,font_weight=600})}
    for i=1,table.maxn(right) do if right[i] then table.insert(row,right[i]) end end
    return ui.Row(row)
end
local function note(id,s,color)
    return label(id,s,CAPTION,color or "onSurfaceVariant",IW)
end
local function empty(id,s)
    return ui.Box{id=id,width=IW,height=line(BODY)+2*sp.xxl,padding_top=sp.xxl,shrink=0,
        ui.Text{id="text",text=s,width=IW,height=line(BODY),font_size=BODY,text_align="center",color="onSurfaceVariant"}}
end
local ROW_X,ROW_Y=sp.compact,sp.sm+sp.hairline/2
local ROW=line(BODY)+line(CAPTION)+2*ROW_Y
-- A settings-style row: icon, title over detail, a trailing node.
local function settings_row(id,name,title,detail,trailing,on,action,ink)
    local tw=IW-2*ROW_X-ICON-2-2*sp.md-(trailing and trailing.w or 0)
    local row={id="row",width=IW-2*ROW_X,height=ROW+sp.sm,gap=sp.md,align="center",
        glyph("icon",name,ICON,on and "primary" or "onSurfaceVariant"),
        ui.Column{id="text",width=tw,gap=0,shrink=0,
            label("title",title,BODY,"onSurface",tw,{font_weight=500}),
            label("detail",detail,CAPTION,ink or "onSurfaceVariant",tw)}}
    if trailing then table.insert(row,trailing.node) end
    return kit.Button{id=id,width=IW,height=ROW+sp.sm,shrink=0,padding=0,padding_left=ROW_X,radius=M.radius.row,
        fill=on and "surfaceContainerHigh" or "transparent",hover_fill=ui.alpha("onSurface",.06),selected=on,focus_ring=true,
        aria_label=title,on_activate=action,ui.Row(row)}
end
local function chip(id,s,ink,border)
    local pad=sp.xs+sp.hairline
    local w=math.ceil(ui.text_width(s,T.fonts.sans,CAPTION))+2*pad+sp.hairline
    return {w=w,node=ui.Box{id=id,width=w,height=line(CAPTION)+sp.xs,radius=sp.xs+sp.hairline,shrink=0,border_color=border or "outlineVariant",border_width=1,
        align="center",justify="center",ui.Text{text=s,width=w-2*pad,height=line(CAPTION),font_size=CAPTION,text_align="center",color=ink or "onSurfaceVariant"}}}
end
local function page(kids)
    kids.id="page";kids.width=CW;kids.padding_left=PAD_X;kids.padding_right=PAD_X;kids.padding_top=PAD_Y;kids.padding_bottom=PAD_Y;kids.gap=sp.md;kids.shrink=0
    return ui.Scroll{id="body",width=CW,grow=1,scroll=ui.bind("ai.scroll"),ui.Column(kids)}
end

-- ---------- chats ----------
local function new_chat()
    local who=provider and provider.label or ""
    local where=(project and project.path or "")..(who~="" and " · "..who or "")
    local can=get("ai.can.start") and provider~=nil and project~=nil
    local inner=IW-sp.lg-sp.compact
    local text=who~="" and ui.tr("bead.panel.ai-new",{provider=who}) or ui.tr("bead.panel.chat-new")
    return kit.Button{id="new-chat",width=IW,height=sp.compact+line(BODY)+sp.sm+BTN+sp.sm,shrink=0,padding=0,padding_left=sp.lg,padding_right=sp.compact,padding_top=sp.compact,
        radius=T.radii.medium,fill="surfaceContainerHigh",hover_fill=ui.tint("surfaceContainerHigh","onSurface",.06),focus_ring=true,aria_label=text,on_activate=can and act("start") or nil,
        ui.Column{id="card",width=inner,gap=sp.sm,
            label("prompt",text,BODY,"onSurfaceVariant",inner),
            ui.Row{id="bar",width=inner,height=BTN,align="center",gap=sp.sm,
                label("where",where,CAPTION,"onSurfaceVariant",nil,{width=0,grow=1}),
                ui.Box{id="send",width=BTN-sp.hairline,height=BTN-sp.hairline,radius=T.radii.full,fill=can and "primary" or "surfaceContainerHighest",shrink=0,align="center",justify="center",
                    glyph("glyph","arrow_upward",ICON,can and "onPrimary" or "onSurfaceVariant")}}}}
end
local DOT=sp.sm
local HALO=DOT+2*(sp.xs-sp.hairline/2)
local GROUP_H=line(CAPTION)+sp.sm+sp.hairline
local function chat_row(item)
    local chipped=chip("provider",string.lower(item.provider or ""))
    local open=ui.tr("bead.panel.launcher-open")
    local open_w=item.running and math.ceil(ui.text_width(open,T.fonts.sans,CAPTION))+2*sp.xs or 0
    local tw=IW-2*ROW_X-HALO-chipped.w-2*sp.compact-(item.running and open_w+sp.compact or 0)
    local can=get("ai.can.resume") and item.resumable
    local row=kit.Button{id="chat-row",width=IW,height=ROW,shrink=0,padding=0,padding_left=ROW_X,padding_top=ROW_Y,padding_bottom=ROW_Y,radius=M.radius.row,
        fill="surface",hover_fill="surfaceContainerHigh",focus_ring=true,aria_label=item.title,on_activate=can and act("resume:"..item.key) or nil,
        ui.Row{id="line",width=IW-2*ROW_X,height=ROW-2*ROW_Y,gap=sp.compact,align="center",
            ui.Box{id="halo",width=HALO,height=HALO,radius=T.radii.full,shrink=0,fill=item.working and ui.alpha("success",.25) or "transparent",align="center",justify="center",
                ui.Box{id="dot",width=DOT,height=DOT,radius=T.radii.full,fill=item.working and "success" or item.running and "primary" or "outlineVariant"}},
            ui.Column{id="detail",width=tw,height=ROW-2*ROW_Y,gap=0,shrink=0,
                label("title",item.title,BODY,"onSurface",tw,{font_weight=500}),
                label("meta",((item.project or "")~="" and item.project.." · " or "")..item.age,CAPTION,"onSurfaceVariant",tw)},
            chipped.node,
            item.running and label("open",open,CAPTION,"primary",open_w,{font_weight=500,text_align="center"}) or nil}}
    if (item.group or "")=="" then return row end
    return ui.Column{id="group",width=IW,height=ROW+GROUP_H,gap=0,shrink=0,
        ui.Box{id="heading",width=IW,height=GROUP_H,padding_left=ROW_X,padding_top=sp.sm,
            ui.Text{text=item.group,width=IW-ROW_X,height=line(CAPTION),font_size=CAPTION,font_weight=600,color="onSurfaceVariant"}},
        row}
end
local function chats()
    local options={}
    for _,p in ipairs(providers) do if p.chat then table.insert(options,{id=p.id,label=p.label,selected=provider~=nil and p.id==provider.id}) end end
    local kids={header(ui.tr("bead.panel.ai-chats"),{
        #options>0 and kit.Segment{id="provider",options=options,on_change="ai.provider",aria_label=ui.tr("bead.panel.ai-provider")} or nil,
        icon_button("refresh","refresh",ui.tr("bead.panel.chat-refresh"),get("ai.connected") and act("refresh") or nil)}),
        new_chat()}
    local rows=get("ai.chats")
    local failed=get("ai.message")~=""
    if failed then table.insert(kids,note("message",get("ai.message"),"error")) end
    if #rows==0 then
        -- After an error the note stands alone: no "Loading chats…" that never ends.
        if not failed or not get("ai.connected") then
            table.insert(kids,empty("empty",get("ai.connected") and (get("ai.loaded") and ui.tr("bead.panel.ai-no-chats") or ui.tr("bead.panel.ai-loading")) or ui.tr("bead.panel.ai-offline")))
        end
    else
        table.insert(kids,ui.Column{id="chats",width=IW,gap=sp.xs,shrink=0,ui.Repeat{id="rows",model=ui.bind("ai.chats"),key="key",render=chat_row}})
    end
    return page(kids)
end

-- ---------- projects ----------
local function projects_page()
    local kids={header(ui.tr("bead.panel.ai-projects"),{icon_button("refresh","refresh",ui.tr("bead.panel.chat-refresh"),get("ai.can.refresh") and act("refresh:projects") or nil)}),
        note("hint",ui.tr("bead.panel.ai-projects-hint"))}
    for i,p in ipairs(projects) do
        local on=project~=nil and p.id==project.id
        table.insert(kids,settings_row("project-"..i,p.remote and "cloud" or "folder",p.label,p.path,
            on and {w=ICON+2,node=glyph("check","check",ICON,"primary")} or nil,on,ui.action("ai.project",p.id)))
    end
    if #projects==0 then table.insert(kids,empty("empty",ui.tr("bead.panel.ai-no-projects"))) end
    return page(kids)
end

-- ---------- accounts ----------
local function accounts_page()
    local kids={header(ui.tr("bead.panel.ai-accounts"),{
        kit.PillButton{id="check",height=BTN,label=ui.tr("bead.panel.ai-check"),glyph="verified_user",compact=true,focus_ring=true,
            label_width=math.ceil(ui.text_width(ui.tr("bead.panel.ai-check"),T.fonts.sans,T.text.caption*4/3)),
            on_activate=get("ai.can.refresh") and act("refresh:accounts") or nil,aria_label=ui.tr("bead.panel.ai-check")}}),
        note("hint",ui.tr("bead.panel.ai-accounts-hint"))}
    local rows=get("ai.accounts")
    for i,a in ipairs(rows) do
        if a.group~="" then
            table.insert(kids,ui.Box{id="group-"..i,width=IW,height=GROUP_H,padding_left=ROW_X,padding_top=sp.sm,shrink=0,
                ui.Text{text=a.group,width=IW-ROW_X,height=line(CAPTION),font_size=CAPTION,font_weight=600,color="onSurfaceVariant"}})
        end
        table.insert(kids,settings_row("account-"..i,"person",a.label,a.status,a.active and chip("active",ui.tr("bead.panel.ai-active"),"primary","primary") or nil,
            false,nil,a.tone))
    end
    if #rows==0 then table.insert(kids,empty("empty",ui.tr("bead.panel.ai-no-accounts"))) end
    return page(kids)
end

-- ---------- usage ----------
local RING,STROKE=sp.huge*3,sp.compact
local function ring(item,ink)
    local fraction=math.clamp(item.percent/100,0,1)
    local w=RING+sp.huge
    local lines=item.detail~="" and 2 or 1
    return ui.Box{id="ring-"..item.id,width=w,height=RING+sp.sm+lines*line(CAPTION),shrink=0,
        ui.Arc{id="track",position="absolute",left=sp.huge/2+STROKE/2,top=STROKE/2,width=RING-STROKE,height=RING-STROKE,
            start_angle=-90,sweep_angle=360,stroke_width=STROKE,color="surfaceContainerHigh"},
        ui.Arc{id="amount",visible=fraction>0,position="absolute",left=sp.huge/2+STROKE/2,top=STROKE/2,width=RING-STROKE,height=RING-STROKE,
            start_angle=-90,sweep_angle=360*fraction,stroke_width=STROKE,color=ink,round_caps=true},
        ui.Text{id="percent",position="absolute",left=sp.huge/2,top=(RING-line(TITLE+4))/2,width=RING,height=line(TITLE+4),
            text=item.value,font_size=TITLE+4,font_weight=600,text_align="center",color="onSurface"},
        ui.Text{id="label",position="absolute",left=0,top=RING+sp.sm,width=w,height=line(CAPTION),text=item.label,font_size=CAPTION,text_align="center",color="onSurface"},
        item.detail~="" and ui.Text{id="resets",position="absolute",left=0,top=RING+sp.sm+line(CAPTION),width=w,height=line(CAPTION),
            text=item.detail,font_size=CAPTION,text_align="center",color="onSurfaceVariant"} or nil}
end
local function usage_page()
    local kids={header(ui.tr("bead.panel.ai-usage"),{icon_button("refresh","refresh",ui.tr("bead.panel.chat-refresh"),get("ai.can.refresh") and act("refresh:usage") or nil)})}
    local rings,lines={},{}
    -- The chosen provider's rings are primary, every other provider's secondary.
    for _,item in ipairs(get("ai.usage")) do
        if type(item.percent)=="number" then
            table.insert(rings,ring(item,provider~=nil and item.provider==provider.id and "primary" or "secondary"))
        else
            table.insert(lines,label("line-"..item.id,item.label,item.head and BODY or CAPTION,item.head and "onSurface" or "onSurfaceVariant",IW,{font_weight=item.head and 500 or 400}))
        end
    end
    if #rings==0 and #lines==0 then table.insert(kids,empty("empty",ui.tr("bead.dashboard.ai-usage-none"))) end
    if #rings>0 then
        rings.id="rings";rings.width=IW;rings.gap=sp.huge;rings.padding_top=sp.xxl;rings.shrink=0;rings.justify="center"
        table.insert(kids,ui.Row(rings))
    end
    if #lines>0 then
        lines.id="lines";lines.width=IW;lines.gap=sp.xs;lines.padding_top=sp.lg;lines.shrink=0
        table.insert(kids,ui.Column(lines))
    end
    return page(kids)
end

local content=PAGE=="projects" and projects_page() or PAGE=="accounts" and accounts_page() or PAGE=="usage" and usage_page() or chats()
return ui.Row{id="ai",width="100%",height="100%",fill="surface",rail(),
    ui.Column{id="main",width=CW,height="100%",clip_children=true,content}}
