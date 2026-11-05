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
local E,BTN,INNER,TABS,TOP=get("chat.edge"),get("chat.button"),get("chat.inner"),get("chat.tabs"),get("chat.top")
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
add(ui.Text{id="title",position="absolute",left=E+T.icons.hero+T.spacing.sm,top=E+T.spacing.xs-1,width=W/2,height=T.icons.hero,
    text=ui.tr("bead.panel.chat-title"),font_size=T.text.headline*4/3-T.spacing.xs/3,color="primary"})
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
    local LT=LTOP+BTN+T.spacing.xs
    local PROJECT_TOP=LT+BTN+T.spacing.xs
    local ROW=BODY+2*CAPTION+T.spacing.md
    local search_top=PROJECT_TOP+BTN+T.spacing.xs
    local recent_top=search_top+BTN+T.spacing.sm
    local list_top=recent_top+CAPTION+T.spacing.xs
    local footer_h=BTN+T.spacing.xs
    local list_bottom=H-E-footer_h-T.spacing.md
    local warning=get("launcher.warning")
    local warning_h=warning~="" and 2*CAPTION or 0
    local rows_top=list_top+warning_h
    local list_h=math.max(0,list_bottom-rows_top)
    local project=get("launcher.project")
    local project_picker_open=get("launcher.project_picker_open")
    local provider=get("launcher.provider")
    local projects=get("launcher.projects")
    local selected_project=""
    for _,item in projects do if item.id==project then selected_project=item.label end end
    add(ui.Row{id="launcher-actions",position="absolute",left=E,top=LTOP,width=INNER,height=BTN,gap=T.spacing.xs,
        button("new-chat",nil,nil,nil,BTN,ui.tr("bead.panel.chat-new"),"add_comment",get("launcher.can_new") and ui.action("chat.do","launcher:new") or nil,true,BODY),
        button("resume",nil,nil,nil,BTN,ui.tr("bead.panel.launcher-resume"),"history",ui.action("chat.do","launcher:latest"),false,BODY)})
    add(ui.Row{id="provider",position="absolute",left=E,top=LT,width=INNER,height=BTN,gap=T.spacing.xs,
        button("codex",nil,nil,nil,BTN,ui.tr("bead.panel.chat-codex"),"",ui.action("chat.do","launcher:provider:codex"),provider=="codex",CAPTION),
        button("claude",nil,nil,nil,BTN,ui.tr("bead.panel.chat-claude"),"",ui.action("chat.do","launcher:provider:claude"),provider=="claude",CAPTION)})
    add(ui.Row{id="project-picker",position="absolute",left=E,top=PROJECT_TOP,width=INNER,height=BTN,gap=T.spacing.xs,align="center",
        ui.Text{id="project-label",text=ui.tr("bead.panel.launcher-project"),width=CAPTION*4,height=BTN,font_size=CAPTION,color="onSurfaceVariant"},
        kit.PillButton{id="selected-project",label=selected_project~="" and selected_project or ui.tr("bead.panel.launcher-choose-project"),
            glyph="arrow_drop_down",width=INNER-CAPTION*4-T.spacing.xs,selected=project_picker_open,
            on_activate=ui.action("chat.do","launcher:toggle-project-picker"),aria_label=ui.tr("bead.panel.launcher-choose-project")}})
    add(kit.TextInput{id="search",position="absolute",left=E,top=search_top,width=INNER,height=BTN,
        text=ui.bind("launcher.search"),placeholder=ui.tr("bead.panel.launcher-search"),padding=T.spacing.sm,
        radius=T.radii.small,font_size=BODY,fill="surfaceContainerHigh",focus_ring=true})
    add(ui.Text{id="recent",position="absolute",left=E,top=recent_top,width=INNER*.7,height=CAPTION+T.spacing.xs,
        text=ui.tr("bead.panel.launcher-recent"),font_size=CAPTION,color="onSurfaceVariant"})
    add(button("refresh",E+INNER-T.spacing.huge-T.spacing.xl, recent_top-T.spacing.xs, T.spacing.huge+T.spacing.xl,BTN,
        ui.tr("bead.panel.chat-refresh"),"refresh",ui.action("chat.do","launcher:refresh"),false,CAPTION))
    if warning~="" then
        add(ui.Text{id="launcher-warning",position="absolute",left=E,top=list_top,width=INNER,height=warning_h,
            text=warning,font_size=CAPTION,color="error"})
    end
    add(launcher{id="recent-chats",scroll=ui.bind("launcher.scroll"),position="absolute",left=E,top=rows_top,
        width=INNER,height=list_h,gap=T.spacing.xs,row_height=ROW,model=ui.bind("launcher.chats"),key="key",
        render=function(item)
            local meta_width=INNER-2*T.spacing.sm
            local active=item.state=="running"
            return kit.Button{id="chat-row",width=INNER,height=ROW,padding=T.spacing.xs+1,radius=T.radii.small,
                fill="surfaceContainer",selected=item.key==get("launcher.selected"),aria_label=item.title,
                on_activate=get("launcher.can_open") and ui.action("chat.do","launcher:open:"..item.key) or nil,
                ui.Box{position="absolute",left=0,top=0,width=T.spacing.hairline+1,height=ROW,
                    radius=T.spacing.hairline,fill="primary",visible=item.key==get("launcher.selected")},
                ui.Column{id="chat-detail",position="absolute",left=T.spacing.sm+T.spacing.xs,top=T.spacing.xs,
                    width=meta_width,height=ROW-2*T.spacing.xs,gap=0,
                    ui.Text{id="title",text=item.title,width=meta_width,height=BODY+2,font_size=BODY-1,color="onSurface"},
                    ui.Text{id="project-provider",text=item.project.."  /  "..item.provider,width=meta_width,height=CAPTION+1,font_size=CAPTION,color="onSurfaceVariant"},
                    ui.Text{id="activity",text=(active and ui.tr("bead.panel.launcher-running") or ui.tr("bead.panel.launcher-idle")).."  "..item.lastActivityLabel,
                        width=meta_width,height=CAPTION+1,font_size=CAPTION-1,color=active and "primary" or "onSurfaceVariant"}}}
        end})
    local empty=get("launcher.empty_state")
    if empty~="" then
        add(ui.Text{id="launcher-message",position="absolute",left=E,top=rows_top+math.max(0,(list_h-2*CAPTION)/2),width=INNER,height=2*CAPTION,
            text=empty,font_size=CAPTION,text_align="center",color="onSurfaceVariant"})
    end
    add(ui.Row{id="secondary",position="absolute",left=E,top=H-E-footer_h,width=INNER,height=BTN,gap=T.spacing.xs,
        button("brain",nil,nil,40,BTN,"","neurology",ui.action("chat.do","launcher:brain"),false,CAPTION,ui.tr("bead.panel.launcher-brain")),
        button("settings",nil,nil,40,BTN,"","settings",ui.action("chat.do","launcher:settings"),false,CAPTION,ui.tr("bead.panel.chat-tab-settings")),
        button("projects",nil,nil,40,BTN,"","folder_open",ui.action("chat.do","launcher:projects"),false,CAPTION,ui.tr("bead.panel.launcher-projects")),
        button("quick-chat",nil,nil,112,BTN,ui.tr("bead.panel.launcher-quick-chat"),"forum",ui.action("chat.do","launcher:quick-chat"),false,CAPTION)})
    if project_picker_open and #projects>0 then
        local PICK_TOP=PROJECT_TOP+BTN
        local PICK_H=math.min(160,H-E-PICK_TOP-T.spacing.sm)
        add(launcher{id="project-options",scroll=ui.bind("launcher.project-scroll"),position="absolute",left=E+CAPTION*4+T.spacing.xs,
            top=PICK_TOP,width=INNER-CAPTION*4-T.spacing.xs,height=PICK_H,gap=T.spacing.xs,row_height=BTN,
            model=ui.bind("launcher.projects"),key="id",render=function(item)
                return kit.Button{id="project-option",width="fill",height=BTN,padding=T.spacing.xs,radius=T.radii.small,
                    fill=item.id==project and "secondaryContainer" or "surfaceContainerHigh",selected=item.id==project,
                    aria_label=item.label,on_activate=ui.action("chat.do","launcher:project:"..item.id),
                    ui.Text{text=item.label,width="fill",height=BTN,font_size=BODY,color="onSurface"}}
            end})
    end
end
return kit.Panel{id="chat",edge=get("view.edge"),open=ui.bind("panel.open"),duration=kit.motion.text,return_duration=.18,return_transition="ease",transition="ease-in-out",fade=false,reveal=not ui.resolve_for_check(ui.bind("view.detached")),
    width=W,height=H,padding=0,radius=T.radii.medium+2,fill="surface",table.unpack(kids)}
