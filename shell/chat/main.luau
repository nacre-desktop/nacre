-- SPDX-License-Identifier: GPL-3.0-only
-- AI sidebar (B0043) composed from kit components. Geometry (edge, button height,
-- tab and content tops, footer positions) is projected from the size scale by the
-- adapter, so this file holds no pixel literals tied to one setup.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local function metric(name,fallback) local value=get(name) return type(value)=="number" and value>0 and value or fallback end
local px=function(pt) return pt*4/3 end
local BODY,CAPTION=px(T.text.body),px(T.text.caption)
local HEADER_TITLE_H,HEADER_META_H=math.ceil(BODY*1.4),math.ceil(CAPTION*1.4)
local HEADER_H=HEADER_TITLE_H+HEADER_META_H+T.spacing.xs/4
local H=metric("chat.height",T.spacing.huge*20)
local section=get("chat.section")
local launcher=require("bead.widgets.kit.virtual-list")
local E=metric("chat.edge",T.spacing.md+T.spacing.xs)
local BTN=metric("chat.button",2*T.spacing.md+2.5*T.spacing.xs)
local INNER=metric("chat.inner",T.spacing.huge*14)
local W=INNER+2*E
local labels={}
for _,item in get("chat.labels") do labels[item.text]=item end
local kids={}
local function add(node) table.insert(kids,node) end
-- Intrinsic QML ActionButton proportions, shared through the kit.
local function button(id,x,y,w,h,label,icon,action,selected,size,aria)
    return kit.PillButton{id=id,position=x and "absolute" or nil,left=x,top=y,width=w,height=h,
        label=label,label_width=labels[label] and (size==CAPTION and labels[label].caption or labels[label].body) or nil,glyph=icon,on_activate=action,selected=selected,compact=size==CAPTION,aria_label=aria or label}
end
local function action(name) return ui.action("chat.do",name) end
if section=="chat" and get("launcher.quick_chat") then
    add(ui.Row{id="header",position="absolute",left=E,top=E,width=INNER,height=HEADER_H,gap=T.spacing.xs/4,align="center",
        button("back",nil,nil,BTN,HEADER_H,"","arrow_back",action("launcher:back"),false,CAPTION,ui.tr("bead.panel.launcher-back")),
        ui.Column{id="heading",width=0,grow=1,justify="center",gap=T.spacing.xs/4,
            ui.Text{id="title",text=ui.bind("chat.title"),font_size=BODY,color="onSurface",height=HEADER_TITLE_H},
            ui.Text{id="subtitle",text=ui.bind("chat.subtitle"),font_size=CAPTION,color="onSurfaceVariant",height=HEADER_META_H}},
        button("overflow",nil,nil,BTN,HEADER_H,"","more_vert",ui.action("chat.menu.toggle",""),false,CAPTION,ui.tr("bead.launcher.actions"))})
else
    add(ui.Row{id="header",position="absolute",left=E,top=E,width=INNER,height=T.icons.hero,gap=T.spacing.xs,align="center",
        ui.Image{id="logo",width=T.icons.hero-T.spacing.xs,height=T.icons.hero-T.spacing.xs,path="chat/assets/nacre-ai.themed.svg",aria_label=ui.tr("bead.panel.chat-title")},
        ui.Text{id="title",width=0,grow=1,text=ui.tr("bead.panel.chat-title"),font_size=BODY,color="onSurface"},
        ui.Row{id="provider",width=T.spacing.huge*5,height=BTN,gap=T.spacing.xs,
            button("claude",nil,nil,(T.spacing.huge*5-T.spacing.xs)/2, BTN,ui.tr("bead.panel.chat-claude"),"",action("launcher:provider:claude"),get("launcher.provider")=="claude",CAPTION),
            button("codex",nil,nil,(T.spacing.huge*5-T.spacing.xs)/2,BTN,ui.tr("bead.panel.chat-codex"),"",action("launcher:provider:codex"),get("launcher.provider")=="codex",CAPTION)},
        not get("view.detached") and button("pin",nil,nil,BTN,BTN,"","push_pin",action("pin"),get("chat.pinned"),CAPTION,ui.tr("bead.panel.chat-pin")) or nil})
end
if section=="chat" and get("launcher.quick_chat") then
    local TT,TH=get("chat.transcript_top"),get("chat.transcript_height")
    local cells=get("chat.cells") or {}
    local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
    local focused=ui.resolve_for_check(ui.bind("bead.focused")) or ""
    local hovered_message,focused_message=nil,nil
    for _,cell in cells do
        local message_id=cell.message and "message-"..tostring(cell.message)
        local copy_id=cell.message and "copy-"..tostring(cell.message)
        if message_id and (string.find(hovered,message_id,1,true) or string.find(hovered,copy_id,1,true)) then hovered_message=cell.message end
        if message_id and (string.find(focused,message_id,1,true) or string.find(focused,copy_id,1,true)) then focused_message=cell.message end
    end
    local copy_message=hovered_message or focused_message
    add(ui.Scroll{id="transcript",position="absolute",left=E,top=TT,width=INNER,height=TH,scroll=ui.bind("chat.scroll"),
        ui.Box{id="extent",width=INNER,height=math.max(get("chat.extent"),1),shrink=0,
            ui.Repeat{id="cells",model=ui.bind("chat.cells"),key="id",render=function(item)
                local copy=item.a and string.sub(item.a,1,5)=="copy:"
                if copy and item.message~=copy_message then
                    return ui.Box{id="hidden",position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,visible=false}
                end
                if item.k=="fill" then
                    if item.message and string.sub(tostring(item.id),-2)==":0" then
                        return kit.Button{id="message-"..tostring(item.message),position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,padding=0,radius=item.r,fill=item.b,hover_fill=item.b,aria_label="Message",on_activate=ui.action("chat.menu.close",""),ui.Box{width=0,height=0}}
                    end
                    return ui.Box{id="fill",position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,radius=item.r,fill=item.b}
                elseif item.k=="text" then
                    return ui.Text{id="text",position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,text=item.t,font_size=item.s,family=item.f,color=item.c}
                end
                if copy then
                    return kit.Button{id="copy-"..tostring(item.message),position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,padding=0,radius=T.radii.full,
                        fill="transparent",hover_fill="surfaceContainerHigh",activate_on_release=true,aria_label=item.t,on_activate=ui.action("chat.do",item.a),
                        ui.Text{text="content_copy",width=item.w,height=item.h,font_size=px(T.icons.small),family=T.fonts.icon,text_align="center",color="onSurfaceVariant"}}
                end
                local pad=math.max(0,math.floor((item.h-item.s*1.4)/2))
                return kit.Button{id="button",position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,padding=0,padding_top=pad,
                    radius=T.radii.full,activate_on_release=true,fill="surfaceContainerHigh",aria_label=item.t,on_activate=ui.action("chat.do",item.a),
                    ui.Text{id="label",text=item.t,width=item.w,height=item.h-pad,font_size=item.s,text_align="center",color="onSurface"}}
            end}}})
    if get("chat.empty") then
        add(ui.Text{id="empty",position="absolute",left=E,top=TT+(TH-HEADER_TITLE_H)/2,width=INNER,height=HEADER_TITLE_H,text=ui.tr("bead.panel.chat-empty"),
            font_size=BODY,text_align="center",color="onSurfaceVariant"})
    end
    if get("chat.status_height")>0 then
        add(ui.Text{id="status",position="absolute",left=E,top=get("chat.status_top"),width=INNER,height=get("chat.status_height"),
            text=ui.bind("chat.status"),font_size=CAPTION,color=get("chat.status_error") and "error" or "onSurfaceVariant"})
    end
    if get("chat.has_question") then
        local can_reply=get("chat.can_reply")
        local RW=INNER-T.spacing.sm
        add(ui.Scroll{id="questions",scroll=ui.bind("chat.qscroll"),position="absolute",left=E,top=get("chat.question_top"),width=INNER,height=get("chat.question_height"),
            ui.Column{id="question-rows",width=RW,gap=T.spacing.xs+T.spacing.hairline/2,
                ui.Repeat{id="rows",model=ui.bind("chat.question_rows"),key="id",render=function(item)
                    if item.k=="text" then
                        return ui.Text{id="prompt",text=item.t,width=RW,height=item.h,font_size=BODY,color="onSurface"}
                    elseif item.k=="option" then
                        return kit.Button{id="option",width=RW,height=item.h,padding=0,padding_top=T.spacing.xs+T.spacing.hairline/2,radius=T.spacing.compact,
                            fill=item.sel and "primary" or "surfaceContainerHigh",aria_label=item.t,on_activate=ui.action("chat.do",item.a),
                            ui.Text{id="label",text=item.t,width=RW-2*T.spacing.sm,height=HEADER_TITLE_H,font_size=BODY-T.spacing.hairline,text_align="center",
                                color=item.sel and "onPrimary" or "onSurface"}}
                    elseif item.k=="input" then
                        return kit.TextInput{id="answer",text=ui.bind("chat.answer."..item.i),placeholder=ui.tr("bead.panel.chat-your-answer"),
                            width=RW,height=item.h,padding=T.spacing.compact,padding_top=T.spacing.xs+T.spacing.hairline/2,padding_bottom=0,radius=T.spacing.compact,font_size=BODY-T.spacing.hairline/2,fill="surfaceContainerHigh"}
                    end
                    return kit.Button{id="reply",width=RW,height=item.h,padding=0,padding_top=T.spacing.sm-T.spacing.hairline,radius=T.spacing.md,
                        fill=can_reply and "primary" or "surfaceContainer",aria_label=ui.tr("bead.panel.chat-reply"),
                        on_activate=can_reply and ui.action("chat.do","reply") or nil,
                        ui.Text{id="label",text=ui.tr("bead.panel.chat-reply"),width=RW-2*T.spacing.sm,height=HEADER_TITLE_H,font_size=BODY-T.spacing.hairline/2,text_align="center",
                            color=can_reply and "onPrimary" or "onSurfaceVariant"}}
                end}}})
    end
    add(kit.Composer{id="composer",position="absolute",left=E,top=get("chat.composer_top"),width=INNER,height=get("chat.composer_height"),padding_right=BTN+T.spacing.xs,
        font_size=BODY,focus_ring=false,text=ui.bind("chat.draft"),placeholder=get("chat.placeholder"),aria_label=get("chat.placeholder"),
        on_activate=ui.action("chat.do","send")})
    add(button("send",E+INNER-BTN-T.spacing.xs,get("chat.composer_top")+T.spacing.xs,BTN,BTN,"","arrow_upward",get("chat.can_send") and ui.action("chat.do","send") or nil,true,CAPTION,ui.tr("bead.panel.chat-send")))
    local AT=get("chat.actions_top")
    local busy=get("chat.busy")
    local actions={}
    -- The published ai.chat contract does not expose file/screenshot attachment actions yet.
    table.insert(actions,button("attach",nil,nil,nil,BTN,"","attach_file",nil,false,CAPTION,ui.tr("bead.panel.chat-attach")))
    table.insert(actions,button("screenshot",nil,nil,nil,BTN,"","screenshot",nil,false,CAPTION,ui.tr("bead.panel.chat-screenshot")))
    if busy then table.insert(actions,button("stop",nil,nil,nil,BTN,"","stop",get("chat.can_stop") and ui.action("chat.do","stop") or nil,false,CAPTION,ui.tr("bead.panel.chat-stop"))) end
    table.insert(actions,button("latest",nil,nil,nil,BTN,"","arrow_downward",ui.action("chat.do","latest"),false,CAPTION,ui.tr("bead.panel.chat-latest")))
    add(ui.Row{id="actions",position="absolute",left=E,top=AT,width=INNER,height=BTN,gap=T.spacing.xs+T.spacing.hairline,table.unpack(actions)})
    local menu_width=T.spacing.huge*6
    if get("chat.menu.open") then
        add(ui.Column{id="overflow-menu",position="absolute",left=E+INNER-menu_width,top=E+HEADER_H,width=menu_width,gap=T.spacing.hairline,
            kit.Button{id="workspace",width=menu_width,height=BTN,padding=T.spacing.xs,radius=T.radii.small,fill="surfaceContainerHigh",aria_label=ui.tr("bead.panel.chat-workspace"),on_activate=get("chat.can_workspace") and ui.action("chat.do","workspace") or nil,
                ui.Text{text=ui.tr("bead.panel.chat-workspace"),width=menu_width-T.spacing.sm,height=BTN,font_size=CAPTION,color="onSurface"}},
            kit.Button{id="new-chat",width=menu_width,height=BTN,padding=T.spacing.xs,radius=T.radii.small,fill="surfaceContainerHigh",aria_label=ui.tr("bead.panel.chat-new"),on_activate=get("chat.can_new") and ui.action("chat.do","new") or nil,
                ui.Text{text=ui.tr("bead.panel.chat-new"),width=menu_width-T.spacing.sm,height=BTN,font_size=CAPTION,color="onSurface"}}})
    end
else
    local TOP=E+T.icons.hero+T.spacing.sm
    local TITLE_H,META_H=math.ceil(BODY*1.4),math.ceil(CAPTION*1.4)
    local ROW=TITLE_H+META_H+T.spacing.sm+T.spacing.hairline
    local warning=get("launcher.warning")
    local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
    local focused=ui.resolve_for_check(ui.bind("bead.focused")) or ""
    add(ui.Row{id="start-actions",position="absolute",left=E,top=TOP,width=INNER,height=BTN,gap=T.spacing.xs,
        kit.PillButton{id="new-chat",width=(INNER-T.spacing.xs)/2,height=BTN,label=ui.tr("bead.panel.chat-new"),glyph="add_comment",on_activate=get("launcher.can_new") and action("launcher:new") or nil,selected=true,aria_label=ui.tr("bead.panel.chat-new")},
        kit.PillButton{id="quick-chat",width=(INNER-T.spacing.xs)/2,height=BTN,label=ui.tr("bead.panel.launcher-quick-chat"),glyph="forum",fill="transparent",hover_fill="surfaceContainerHigh",on_activate=action("launcher:quick-chat"),aria_label=ui.tr("bead.panel.launcher-quick-chat")}})
    local search_top=TOP+BTN+T.spacing.xs
    add(kit.TextInput{id="search",position="absolute",left=E,top=search_top,width=INNER,height=BTN,
        text=ui.bind("launcher.search"),placeholder=ui.tr("bead.panel.launcher-search"),padding=T.spacing.sm,padding_right=BTN,
        radius=T.radii.small,font_size=BODY,fill="surfaceContainerHigh",focus_ring=true})
    local refresh_label=ui.tr("bead.panel.chat-refresh")
    add(button("refresh",E+INNER-BTN,search_top,BTN,BTN,"","refresh",action("launcher:refresh"),false,CAPTION,refresh_label))
    local tooltip_height=CAPTION+2*T.spacing.sm
    local rows_top=search_top+BTN+T.spacing.sm
    local warning_h=warning~="" and META_H+T.spacing.xs or 0
    if warning_h>0 then add(ui.Text{id="launcher-warning",position="absolute",left=E,top=rows_top,width=INNER,height=warning_h,text=warning,font_size=CAPTION,color="onSurfaceVariant"}) end
    rows_top+=warning_h
    local footer_top=H-E-BTN-T.spacing.md
    local list_h=math.max(0,footer_top-rows_top-T.spacing.xs)
    local chats=get("launcher.chats") or {}
    local newest=get("launcher.search")=="" and chats[1] and chats[1].key or ""
    add(launcher{id="recent-chats",scroll=ui.bind("launcher.scroll"),position="absolute",left=E,top=rows_top,width=INNER,height=list_h,
        gap=T.spacing.xs,row_height=ROW,model=ui.bind("launcher.chats"),key="key",render=function(item)
            local inset=T.spacing.xs
            local active=item.state=="running"
            return kit.Button{id="chat-row",width=INNER,height=ROW,padding=inset,radius=T.radii.small,fill="surface",
                hover_fill="surfaceContainerHigh",selected_fill="surfaceContainerHigh",selected=item.key==get("launcher.selected"),aria_label=item.title,
                on_activate=get("launcher.can_open") and action("launcher:open:"..item.key) or nil,
                ui.Column{id="chat-detail",width=INNER-2*inset,height=ROW-2*inset,gap=T.spacing.hairline,
                    ui.Row{id="title-row",width=INNER-2*inset,height=TITLE_H,gap=T.spacing.xs,align="center",
                        ui.Text{id="title",text=item.title,width=0,grow=1,height=TITLE_H,font_size=BODY,color="onSurface"},
                        item.key==newest and kit.Button{id="continue-chip",width=ui.text_width(ui.tr("bead.prompt.continue"),T.fonts.sans,CAPTION)+2*T.spacing.xs,height=META_H,padding=0,radius=T.radii.full,fill="surfaceContainerHigh",
                            aria_label=ui.tr("bead.panel.launcher-resume"),on_activate=get("launcher.can_latest") and action("launcher:latest") or nil,
                            ui.Text{text=ui.tr("bead.prompt.continue"),width=ui.text_width(ui.tr("bead.prompt.continue"),T.fonts.sans,CAPTION)+2*T.spacing.xs,height=META_H,font_size=CAPTION,text_align="center",color="primary"}} or nil},
                    ui.Row{id="meta",width=INNER-2*inset,height=META_H,gap=T.spacing.xs,align="center",
                        ui.Box{id="running-dot",width=T.spacing.xs,height=T.spacing.xs,radius=T.radii.full,fill=active and "primary" or "outlineVariant"},
                        ui.Text{id="details",text=item.provider.." · "..(active and ui.tr("bead.panel.launcher-running") or ui.tr("bead.panel.launcher-idle")).." · "..item.lastActivityLabel,
                            width=INNER-2*inset-T.spacing.sm,height=META_H,font_size=CAPTION,color="onSurfaceVariant"}}}}
        end})
    local empty=get("launcher.empty_state")
    if empty~="" then add(ui.Text{id="launcher-message",position="absolute",left=E,top=rows_top+math.max(0,(list_h-META_H)/2),width=INNER,height=META_H,text=empty,font_size=CAPTION,text_align="center",color="onSurfaceVariant"}) end
    add(ui.Row{id="secondary",position="absolute",left=E,top=footer_top,width=INNER,height=BTN,gap=T.spacing.xs,
        kit.PillButton{id="brain",width=(INNER-T.spacing.xs)/2,height=BTN,label=ui.tr("bead.panel.launcher-brain"),glyph="neurology",compact=true,fill="transparent",hover_fill="surfaceContainerHigh",on_activate=action("launcher:brain"),aria_label=ui.tr("bead.panel.launcher-brain")},
        kit.PillButton{id="settings",width=(INNER-T.spacing.xs)/2,height=BTN,label=ui.tr("bead.panel.chat-tab-settings"),glyph="settings",compact=true,fill="transparent",hover_fill="surfaceContainerHigh",on_activate=action("launcher:settings"),aria_label=ui.tr("bead.panel.chat-tab-settings")}})
    local footer_label=nil
    if string.find(hovered,"brain",1,true) or string.find(focused,"brain",1,true) then footer_label=ui.tr("bead.panel.launcher-brain") end
    if string.find(hovered,"settings",1,true) or string.find(focused,"settings",1,true) then footer_label=ui.tr("bead.panel.chat-tab-settings") end
    if footer_label then
        local w=ui.text_width(footer_label,T.fonts.sans,CAPTION)+2*T.spacing.sm
        add(kit.Tooltip{id="footer-tooltip",position="absolute",edge="top",bar_size=footer_top-tooltip_height-2*T.spacing.sm,
            left=E+(INNER-w)/2,width=w,height=tooltip_height,padding=T.spacing.xs,visible=true,
            ui.Text{text=footer_label,width="fill",height=META_H,font_size=CAPTION,color="onSurface"}})
    end
    if string.find(hovered,"refresh",1,true) or string.find(focused,"refresh",1,true) then
        local w=ui.text_width(refresh_label,T.fonts.sans,CAPTION)+2*T.spacing.sm
        add(kit.Tooltip{id="refresh-tooltip",position="absolute",edge="top",bar_size=search_top-tooltip_height-2*T.spacing.sm,
            left=E+INNER-w,width=w,height=tooltip_height,padding=T.spacing.xs,visible=true,
            ui.Text{text=refresh_label,width="fill",height=META_H,font_size=CAPTION,color="onSurface"}})
    end
end
return kit.Panel{id="chat",edge=get("view.edge"),open=ui.bind("panel.open"),duration=kit.motion.text,return_duration=.18,return_transition="ease",transition="ease-in-out",fade=false,reveal=not ui.resolve_for_check(ui.bind("view.detached")),
    width=W,height=H,padding=0,radius=T.radii.medium+T.spacing.hairline,fill="surface",table.unpack(kids)}
