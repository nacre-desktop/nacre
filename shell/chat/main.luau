-- SPDX-License-Identifier: GPL-3.0-only
-- AI sidebar (B0043) composed from kit components. Geometry (edge, button height,
-- tab and content tops, footer positions) is projected from the size scale by the
-- adapter, so this file holds no pixel literals tied to one setup.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local px=function(pt) return pt*4/3 end
local BODY,CAPTION=px(T.text.body),px(T.text.caption)
local H=get("chat.height")
local section=get("chat.section")
local launcher=require("bead.widgets.kit.virtual-list")
local E,BTN,INNER,TOP=get("chat.edge"),get("chat.button"),get("chat.inner"),get("chat.top")
local W=INNER+2*E
local ICON=px(T.icons.medium)
local labels={}
for _,item in get("chat.labels") do labels[item.text]=item end
local kids={}
local function add(node) table.insert(kids,node) end
-- Intrinsic QML ActionButton proportions, shared through the kit.
local function button(id,x,y,w,h,label,icon,action,selected,size,aria)
    return kit.PillButton{id=id,position=x and "absolute" or nil,left=x,top=y,width=w,height=h,
        label=label,label_width=labels[label] and (size==CAPTION and labels[label].caption or labels[label].body) or nil,glyph=icon,on_activate=action,selected=selected,compact=size==CAPTION,aria_label=aria or label}
end
add(ui.Image{id="logo",position="absolute",left=E,top=E+T.spacing.hairline,width=T.icons.hero-T.spacing.hairline,height=T.icons.hero-T.spacing.hairline,
    path="chat/assets/nacre-ai.themed.svg",aria_label=ui.tr("bead.panel.chat-title")})
add(ui.Text{id="title",position="absolute",left=E+T.icons.hero+T.spacing.sm,top=E+T.spacing.xs,width=W/2,height=T.icons.hero,
    text=ui.tr("bead.panel.chat-title"),font_size=px(T.text.headline),color="onSurface"})
local PIN_W=T.spacing.huge+ICON+T.spacing.xs+T.spacing.hairline/2
if not get("view.detached") then
    add(button("pin",W-E-PIN_W,E,PIN_W,BTN,"","push_pin",ui.action("chat.do","pin"),get("chat.pinned"),BODY,ui.tr("bead.panel.chat-pin")))
end
if section=="chat" and get("launcher.quick_chat") then
    add(ui.Row{id="tools",position="absolute",left=E,top=TOP,width=INNER,height=BTN,gap=T.spacing.sm,
        button("new",nil,nil,nil,BTN,ui.tr("bead.panel.chat-new"),"add_comment",get("chat.can_new") and ui.action("chat.do","new") or nil,false,BODY),
        button("workspace",nil,nil,nil,BTN,ui.tr("bead.panel.chat-workspace"),"open_in_new",get("chat.can_workspace") and ui.action("chat.do","workspace") or nil,false,BODY),
        button("launcher",nil,nil,nil,BTN,ui.tr("bead.panel.launcher-back"),"arrow_back",ui.action("chat.do","launcher:back"),false,CAPTION)})
    add(ui.Text{id="heading",position="absolute",left=E,top=TOP+BTN+T.spacing.sm-1,width=INNER,height=CAPTION*1.5,text=ui.bind("chat.heading"),
        font_size=CAPTION,color="onSurfaceVariant"})
    local TT,TH=get("chat.transcript_top"),get("chat.transcript_height")
    add(kit.Transcript{id="transcript",position="absolute",left=E,top=TT,width=INNER,height=TH,scroll=ui.bind("chat.scroll"),
        cells=ui.bind("chat.cells"),extent=get("chat.extent"),action_radius=T.radii.full,action_release=true,on_action="chat.do"})
    if get("chat.empty") then
        add(ui.Text{id="empty",position="absolute",left=E,top=TT+TH/2-12,width=INNER,height=24,text=ui.tr("bead.panel.chat-empty"),
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
            ui.Column{id="question-rows",width=RW,gap=T.spacing.xs+1,
                ui.Repeat{id="rows",model=ui.bind("chat.question_rows"),key="id",render=function(item)
                    if item.k=="text" then
                        return ui.Text{id="prompt",text=item.t,width=RW,height=item.h,font_size=BODY,color="onSurface"}
                    elseif item.k=="option" then
                        return kit.Button{id="option",width=RW,height=item.h,padding=0,padding_top=T.spacing.xs+1,radius=T.spacing.compact,
                            fill=item.sel and "primary" or "surfaceContainerHigh",aria_label=item.t,on_activate=ui.action("chat.do",item.a),
                            ui.Text{id="label",text=item.t,width=RW-2*T.spacing.sm,height=24,font_size=BODY-2,text_align="center",
                                color=item.sel and "onPrimary" or "onSurface"}}
                    elseif item.k=="input" then
                        return kit.TextInput{id="answer",text=ui.bind("chat.answer."..item.i),placeholder=ui.tr("bead.panel.chat-your-answer"),
                            width=RW,height=item.h,padding=T.spacing.compact,padding_top=T.spacing.xs+1,padding_bottom=0,radius=T.spacing.compact,font_size=BODY-1,fill="surfaceContainerHigh"}
                    end
                    return kit.Button{id="reply",width=RW,height=item.h,padding=0,padding_top=T.spacing.sm-2,radius=T.spacing.md,
                        fill=can_reply and "primary" or "surfaceContainer",aria_label=ui.tr("bead.panel.chat-reply"),
                        on_activate=can_reply and ui.action("chat.do","reply") or nil,
                        ui.Text{id="label",text=ui.tr("bead.panel.chat-reply"),width=RW-2*T.spacing.sm,height=24,font_size=BODY-1,text_align="center",
                            color=can_reply and "onPrimary" or "onSurfaceVariant"}}
                end}}})
    end
    add(kit.Composer{id="composer",position="absolute",left=E,top=get("chat.composer_top"),width=INNER,height=get("chat.composer_height"),
        font_size=BODY,focus_ring=false,text=ui.bind("chat.draft"),placeholder=get("chat.placeholder"),aria_label=get("chat.placeholder"),
        on_activate=ui.action("chat.do","send")})
    local AT=get("chat.actions_top")
    local busy=get("chat.busy")
    local actions={button("send",nil,nil,nil,BTN,ui.tr("bead.panel.chat-send"),"arrow_upward",get("chat.can_send") and ui.action("chat.do","send") or nil,true,CAPTION)}
    -- The published ai.chat contract does not expose file/screenshot attachment actions yet.
    table.insert(actions,button("attach",nil,nil,nil,BTN,ui.tr("bead.panel.chat-attach"),"attach_file",nil,false,CAPTION))
    table.insert(actions,button("screenshot",nil,nil,nil,BTN,busy and "" or ui.tr("bead.panel.chat-screenshot"),"screenshot",nil,false,CAPTION))
    if busy then table.insert(actions,button("stop",nil,nil,nil,BTN,"","stop",get("chat.can_stop") and ui.action("chat.do","stop") or nil,false,CAPTION,ui.tr("bead.panel.chat-stop"))) end
    table.insert(actions,button("latest",nil,nil,nil,BTN,ui.tr("bead.panel.chat-latest"),"arrow_downward",ui.action("chat.do","latest"),false,CAPTION))
    add(ui.Row{id="actions",position="absolute",left=E,top=AT,width=INNER,height=BTN,gap=T.spacing.xs+T.spacing.hairline,table.unpack(actions)})
else
    local LTOP=E+T.icons.hero+T.spacing.sm
    local PROVIDER_H=BTN-T.spacing.xs
    local ROW=BODY+CAPTION+T.spacing.md
    local provider_top=LTOP+BTN+T.spacing.xs
    local resume_top=provider_top+PROVIDER_H+T.spacing.xs
    local search_top=resume_top+BTN+T.spacing.xs
    local list_top=search_top+BTN+T.spacing.sm
    local footer_h=BTN+T.spacing.xs
    local list_bottom=H-E-footer_h-T.spacing.md
    local warning=get("launcher.warning")
    local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
    local warning_h=warning~="" and CAPTION+T.spacing.xs or 0
    local rows_top=list_top+warning_h
    local list_h=math.max(0,list_bottom-rows_top)
    local provider=get("launcher.provider")
    add(button("new-chat",E,LTOP,INNER,BTN,ui.tr("bead.panel.chat-new"),"add_comment",get("launcher.can_new") and ui.action("chat.do","launcher:new") or nil,true,BODY))
    add(ui.Row{id="provider",position="absolute",left=E,top=provider_top,width=INNER,height=PROVIDER_H,gap=T.spacing.xs,
        button("claude",nil,nil,INNER/2-T.spacing.xs/2,PROVIDER_H,ui.tr("bead.panel.chat-claude"),"",ui.action("chat.do","launcher:provider:claude"),provider=="claude",CAPTION),
        button("codex",nil,nil,INNER/2-T.spacing.xs/2,PROVIDER_H,ui.tr("bead.panel.chat-codex"),"",ui.action("chat.do","launcher:provider:codex"),provider=="codex",CAPTION)})
    local resume_label=ui.tr("bead.panel.launcher-resume")
    add(kit.Button{id="resume",position="absolute",left=E,top=resume_top,width=INNER,height=BTN,padding=0,
        fill="transparent",hover_fill="surfaceContainerHigh",focus_ring=true,aria_label=resume_label,
        on_activate=get("launcher.can_latest") and ui.action("chat.do","launcher:latest") or nil,
        ui.Text{id="resume-label",text=resume_label,width=INNER,height=BTN,font_size=BODY,color="primary"}})
    add(kit.TextInput{id="search",position="absolute",left=E,top=search_top,width=INNER-BTN-T.spacing.xs,height=BTN,
        text=ui.bind("launcher.search"),placeholder=ui.tr("bead.panel.launcher-search"),padding=T.spacing.sm,
        radius=T.radii.small,font_size=BODY,fill="surfaceContainerHigh",focus_ring=true})
    local refresh_label=ui.tr("bead.panel.chat-refresh")
    add(button("refresh",E+INNER-BTN,search_top,BTN,BTN,"","refresh",ui.action("chat.do","launcher:refresh"),false,CAPTION,refresh_label))
    add(kit.Tooltip{id="refresh-tooltip",position="absolute",edge="top",bar_size=-T.spacing.sm,
        left=E+INNER-BTN,width=ui.text_width(refresh_label,"IBM Plex Sans",CAPTION)+2*T.spacing.sm,height=CAPTION+2*T.spacing.sm,
        padding=T.spacing.xs,visible=string.find(hovered,"refresh",1,true)~=nil,
        ui.Text{text=refresh_label,width="fill",height=CAPTION+T.spacing.xs,font_size=CAPTION,color="onSurface"}})
    if warning~="" then
        add(ui.Text{id="launcher-warning",position="absolute",left=E,top=list_top,width=INNER,height=warning_h,
            text=warning,font_size=CAPTION,color="onSurfaceVariant"})
    end
    add(launcher{id="recent-chats",scroll=ui.bind("launcher.scroll"),position="absolute",left=E,top=rows_top,
        width=INNER,height=list_h,gap=T.spacing.xs,row_height=ROW,model=ui.bind("launcher.chats"),key="key",
        render=function(item)
            local row_inset=T.spacing.sm
            local meta_width=INNER-2*row_inset-T.spacing.xs-T.spacing.xs
            local active=item.state=="running"
            return kit.Button{id="chat-row",width=INNER,height=ROW,padding=row_inset,radius=T.radii.small,
                fill="surface",selected=item.key==get("launcher.selected"),aria_label=item.title,
                on_activate=get("launcher.can_open") and ui.action("chat.do","launcher:open:"..item.key) or nil,
                ui.Box{position="absolute",left=row_inset,top=ROW-T.spacing.xs,width=meta_width,height=T.spacing.hairline,
                    radius=T.radii.full,fill="outlineVariant",visible=item.key~=get("launcher.selected")},
                ui.Column{id="chat-detail",position="absolute",left=row_inset,top=T.spacing.xs,
                    width=meta_width,height=ROW-2*T.spacing.xs,gap=T.spacing.hairline,
                    ui.Text{id="title",text=item.title,width=meta_width,height=BODY,font_size=BODY,color="onSurface"},
                    ui.Row{id="meta",width=meta_width,height=CAPTION,gap=T.spacing.xs,align="center",
                        ui.Box{id="running-dot",width=T.spacing.xs,height=T.spacing.xs,radius=T.radii.full,fill="primary",visible=active},
                        ui.Text{id="details",text=item.provider.." · "..(active and ui.tr("bead.panel.launcher-running") or ui.tr("bead.panel.launcher-idle")).." · "..item.lastActivityLabel,
                            width=meta_width-T.spacing.sm,height=CAPTION,font_size=CAPTION,color="onSurfaceVariant"}}}}
        end})
    local empty=get("launcher.empty_state")
    if empty~="" then
        add(ui.Text{id="launcher-message",position="absolute",left=E,top=rows_top+math.max(0,(list_h-CAPTION)/2),width=INNER,height=CAPTION,
            text=empty,font_size=CAPTION,text_align="center",color="onSurfaceVariant"})
    end
    local footer_top=H-E-footer_h
    local footer_actions={
        {id="brain",label=ui.tr("bead.panel.launcher-brain"),glyph="neurology",action="launcher:brain"},
        {id="settings",label=ui.tr("bead.panel.chat-tab-settings"),glyph="settings",action="launcher:settings"},
        {id="quick-chat",label=ui.tr("bead.panel.launcher-quick-chat"),glyph="forum",action="launcher:quick-chat"},
    }
    local footer_buttons={id="secondary",position="absolute",left=E,top=footer_top,width=INNER,height=BTN,gap=T.spacing.xs}
    local footer_tooltip=""
    for _,item in footer_actions do
        table.insert(footer_buttons,button(item.id,nil,nil,nil,BTN,item.label,item.glyph,ui.action("chat.do",item.action),false,CAPTION,item.label))
        if string.find(hovered,item.id,1,true)~=nil then footer_tooltip=item.label end
    end
    add(ui.Row(footer_buttons))
    if footer_tooltip~="" then
        local tooltip_width=ui.text_width(footer_tooltip,"IBM Plex Sans",CAPTION)+2*T.spacing.sm
        add(kit.Tooltip{id="footer-tooltip",position="absolute",edge="top",bar_size=-T.spacing.sm,
            left=E+(INNER-tooltip_width)/2,width=tooltip_width,height=CAPTION+2*T.spacing.sm,padding=T.spacing.xs,
            visible=true,ui.Text{text=footer_tooltip,width="fill",height=CAPTION+T.spacing.xs,font_size=CAPTION,color="onSurface"}})
    end
end
return kit.Panel{id="chat",edge=get("view.edge"),open=ui.bind("panel.open"),duration=kit.motion.text,return_duration=.18,return_transition="ease",transition="ease-in-out",fade=false,reveal=not ui.resolve_for_check(ui.bind("view.detached")),
    width=W,height=H,padding=0,radius=T.radii.medium+2,fill="surface",table.unpack(kids)}
