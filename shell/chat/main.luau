-- SPDX-License-Identifier: GPL-3.0-only
-- AI sidebar (B0043, D7 design review 2026-11-13) composed from kit components. Geometry
-- (edge, button height, transcript and composer tops) is projected from the size scale by
-- the adapter, so this file holds no pixel literals tied to one setup. The launcher and
-- conversation views are plain functions of the stores so the D8 app can host them too.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local function metric(name,fallback) local value=get(name) return type(value)=="number" and value>0 and value or fallback end
local px=function(pt) return pt*4/3 end
local BODY,CAPTION=px(T.text.body),px(T.text.caption)
local MONO=CAPTION-T.spacing.hairline
local TITLE_H,META_H=math.ceil(BODY*1.4),math.ceil(CAPTION*1.4)
local HEADER_H=TITLE_H+META_H+T.spacing.xs/4
local H=metric("chat.height",T.spacing.huge*20)
local section=get("chat.section")
local launcher=require("bead.widgets.kit.virtual-list")
local E=metric("chat.edge",T.spacing.md+T.spacing.xs)
local BTN=metric("chat.button",2*T.spacing.md+2.5*T.spacing.xs)
local INNER=metric("chat.inner",T.spacing.huge*14)
local W=INNER+2*E
local GAP=T.spacing.md
local CARD_R=T.radii.medium
local labels={}
for _,item in get("chat.labels") do labels[item.text]=item end
local kids={}
local function add(node) table.insert(kids,node) end
local function button(id,x,y,w,h,label,icon,action,selected,size,aria)
    return kit.PillButton{id=id,position=x and "absolute" or nil,left=x,top=y,width=w,height=h,
        label=label,label_width=labels[label] and (size==CAPTION and labels[label].caption or labels[label].body) or nil,glyph=icon,on_activate=action,selected=selected,compact=size==CAPTION,aria_label=aria or label}
end
-- Round icon button (the review's `ib`): transparent at rest, container-high on hover, primary while selected.
local function icon_button(id,icon,action,aria,selected,x,y)
    return kit.PillButton{id=id,position=x and "absolute" or nil,left=x,top=y,width=BTN,height=BTN,glyph=icon,on_activate=action,compact=true,aria_label=aria,focus_ring=true,selected=selected,
        fill=not selected and "transparent" or nil,hover_fill=not selected and (action and "surfaceContainerHigh" or "transparent") or nil}
end
local function action(name) return ui.action("chat.do",name) end
local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
local focused=ui.resolve_for_check(ui.bind("bead.focused")) or ""
local function cued(id) return string.find(hovered,id,1,true)~=nil or string.find(focused,id,1,true)~=nil end
-- The composer card: the field on top, its actions (left) and send (right) on one row below.
-- Siblings, not children, of the card so `chat.composer` / `chat.actions.*` stay the focus and test ids.
-- The field's text width (INNER - 2 sm - 2 xs) is the width the adapter measures the draft at;
-- the caret marks its focus (a ring would hug the field inside the card).
local function compose(top,field_h,placeholder,on_send,can_send,left,right)
    local inner=INNER-2*T.spacing.sm
    local row_top=top+T.spacing.sm+field_h+T.spacing.xs
    table.insert(right,kit.PillButton{id="send",width=BTN,height=BTN,glyph="arrow_upward",selected=true,compact=true,focus_ring=true,
        on_activate=can_send and on_send or nil,aria_label=ui.tr("bead.panel.chat-send")})
    add(ui.Box{id="compose",position="absolute",left=E,top=top,width=INNER,height=row_top+BTN+T.spacing.sm-top,radius=CARD_R,fill="surfaceContainerHigh"})
    add(kit.Composer{id="composer",position="absolute",left=E+T.spacing.sm,top=top+T.spacing.sm,width=inner,height=field_h,padding=T.spacing.xs,
        radius=T.spacing.sm,fill="transparent",focus_ring=false,font_size=BODY,text=ui.bind("chat.draft"),placeholder=placeholder,aria_label=placeholder,on_activate=on_send})
    add(ui.Row{id="actions",position="absolute",left=E+T.spacing.sm,top=row_top,width=inner,height=BTN,gap=T.spacing.xs,align="center",
        ui.Row{id="left",width=0,grow=1,height=BTN,gap=T.spacing.xs,align="center",table.unpack(left)},
        table.unpack(right)})
end
-- N0234: without the optional AI package the panel only says so (no provider or chat controls).
local missing=get("chat.missing")
local quick=section=="chat" and get("launcher.quick_chat") and not missing
local busy=get("chat.busy")
if quick then
    add(ui.Row{id="header",position="absolute",left=E,top=E,width=INNER,height=HEADER_H,gap=T.spacing.xs,align="center",
        icon_button("back","arrow_back",action("launcher:back"),ui.tr("bead.panel.launcher-back")),
        ui.Column{id="heading",width=0,grow=1,justify="center",gap=T.spacing.xs/4,
            ui.Text{id="title",text=ui.bind("chat.title"),font_size=BODY,font_weight=500,color="onSurface",height=TITLE_H},
            ui.Row{id="meta",width="fill",height=META_H,gap=T.spacing.xs+T.spacing.hairline/2,align="center",
                -- D16: the review's 1.6 s attention pulse waits on the kit (N0237 kit ask 3).
                ui.Box{id="working-dot",width=T.spacing.sm-T.spacing.hairline/2,height=T.spacing.sm-T.spacing.hairline/2,radius=T.radii.full,fill="success",visible=busy},
                ui.Text{id="subtitle",text=ui.bind("chat.subtitle"),width=0,grow=1,font_size=CAPTION,color="onSurfaceVariant",height=META_H}}},
        icon_button("overflow","more_vert",ui.action("chat.menu.toggle",""),ui.tr("bead.launcher.actions"))})
else
    add(ui.Row{id="header",position="absolute",left=E,top=E,width=INNER,height=BTN,gap=T.spacing.sm,align="center",
        ui.Image{id="logo",width=T.icons.xlarge,height=T.icons.xlarge,path="chat/assets/nacre-ai.themed.svg",aria_label=ui.tr("bead.panel.chat-title")},
        ui.Text{id="title",width=0,grow=1,text=ui.tr("bead.panel.chat-title"),font_size=BODY,font_weight=600,color="onSurface"},
        -- Segmented provider switch: one pill, the chosen provider filled.
        ui.Row{id="provider",height=BTN,gap=T.spacing.hairline,padding=T.spacing.hairline+1,radius=T.radii.full,fill="surfaceContainerHigh",align="center",visible=not missing,
            button("codex",nil,nil,nil,BTN-2*(T.spacing.hairline+1),ui.tr("bead.panel.chat-codex"),"",action("launcher:provider:codex"),get("launcher.provider")=="codex",CAPTION),
            button("claude",nil,nil,nil,BTN-2*(T.spacing.hairline+1),ui.tr("bead.panel.chat-claude"),"",action("launcher:provider:claude"),get("launcher.provider")=="claude",CAPTION)},
        not get("view.detached") and icon_button("pin","push_pin",action("pin"),ui.tr("bead.panel.chat-pin"),get("chat.pinned")==true) or nil})
end
if missing then
    local top=(H-TITLE_H-T.spacing.xs-META_H)/2
    add(ui.Text{id="missing-title",position="absolute",left=E,top=top,width=INNER,height=TITLE_H,text=ui.tr("bead.panel.chat-missing"),
        font_size=BODY,text_align="center",color="onSurface"})
    add(ui.Text{id="missing-hint",position="absolute",left=E,top=top+TITLE_H+T.spacing.xs,width=INNER,height=META_H,text=ui.tr("bead.panel.chat-missing-hint"),
        font_size=CAPTION,text_align="center",color="onSurfaceVariant"})
elseif quick then
    local TT,TH=get("chat.transcript_top"),get("chat.transcript_height")
    local cells=get("chat.cells") or {}
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
        add(ui.Text{id="empty",position="absolute",left=E,top=TT+(TH-TITLE_H)/2,width=INNER,height=TITLE_H,text=ui.tr("bead.panel.chat-empty"),
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
                            ui.Text{id="label",text=item.t,width=RW-2*T.spacing.sm,height=TITLE_H,font_size=BODY-T.spacing.hairline,text_align="center",
                                color=item.sel and "onPrimary" or "onSurface"}}
                    elseif item.k=="input" then
                        return kit.TextInput{id="answer",text=ui.bind("chat.answer."..item.i),placeholder=ui.tr("bead.panel.chat-your-answer"),
                            width=RW,height=item.h,padding=T.spacing.compact,padding_top=T.spacing.xs+T.spacing.hairline/2,padding_bottom=0,radius=T.spacing.compact,font_size=BODY-T.spacing.hairline/2,fill="surfaceContainerHigh"}
                    end
                    return kit.Button{id="reply",width=RW,height=item.h,padding=0,padding_top=T.spacing.sm-T.spacing.hairline,radius=T.spacing.md,
                        fill=can_reply and "primary" or "surfaceContainer",aria_label=ui.tr("bead.panel.chat-reply"),
                        on_activate=can_reply and ui.action("chat.do","reply") or nil,
                        ui.Text{id="label",text=ui.tr("bead.panel.chat-reply"),width=RW-2*T.spacing.sm,height=TITLE_H,font_size=BODY-T.spacing.hairline/2,text_align="center",
                            color=can_reply and "onPrimary" or "onSurfaceVariant"}}
                end}}})
    end
    -- The published ai.chat contract exposes neither attachments nor the remote device yet.
    local left={icon_button("attach","attach_file",nil,ui.tr("bead.panel.chat-attach")),icon_button("phone","smartphone",nil,ui.tr("bead.panel.chat-phone"))}
    local right={}
    if busy then
        local STOP=T.spacing.xxl+T.spacing.xs
        table.insert(right,kit.Button{id="stop",width=STOP,height=STOP,padding=0,radius=T.radii.full,fill="surfaceContainerHighest",
            hover_fill=ui.tint("surfaceContainerHighest","onSurface",.08),activate_on_release=true,
            on_activate=get("chat.can_stop") and action("stop") or nil,aria_label=ui.tr("bead.panel.chat-stop"),
            ui.Text{text="stop",width=STOP,height=STOP,font_size=px(T.icons.small),family=T.fonts.icon,text_align="center",color="error"}})
    end
    compose(get("chat.composer_top"),get("chat.composer_height"),get("chat.placeholder"),action("send"),get("chat.can_send"),left,right)
    local menu_width=T.spacing.huge*6
    if get("chat.menu.open") then
        add(ui.Column{id="overflow-menu",position="absolute",left=E+INNER-menu_width,top=E+HEADER_H,width=menu_width,gap=T.spacing.hairline,
            kit.Button{id="workspace",width=menu_width,height=BTN,padding=T.spacing.xs,radius=T.radii.small,fill="surfaceContainerHigh",aria_label=ui.tr("bead.panel.chat-workspace"),on_activate=get("chat.can_workspace") and ui.action("chat.do","workspace") or nil,
                ui.Text{text=ui.tr("bead.panel.chat-workspace"),width=menu_width-T.spacing.sm,height=BTN,font_size=CAPTION,color="onSurface"}},
            kit.Button{id="new-chat",width=menu_width,height=BTN,padding=T.spacing.xs,radius=T.radii.small,fill="surfaceContainerHigh",aria_label=ui.tr("bead.panel.chat-new"),on_activate=get("chat.can_new") and ui.action("chat.do","new") or nil,
                ui.Text{text=ui.tr("bead.panel.chat-new"),width=menu_width-T.spacing.sm,height=BTN,font_size=CAPTION,color="onSurface"}}})
    end
else
    -- Launcher: composer first, then the grouped chat list, then "New chat in project" + Brain/Settings.
    local provider=get("launcher.provider")
    local ask=ui.tr("bead.panel.chat-ask",{provider=ui.tr(provider=="claude" and "bead.panel.chat-claude" or "bead.panel.chat-codex")})
    local FIELD=TITLE_H+2*T.spacing.xs
    local COMPOSE_H=T.spacing.sm+FIELD+T.spacing.xs+BTN+T.spacing.sm
    local compose_top=E+BTN+GAP
    -- Owner 2026-11-14: send opens the quick chat even with an empty draft, and a terminal button beside it
    -- starts a terminal chat with the header's provider (the pre-D7 one-click New chat).
    compose(compose_top,FIELD,ask,action("launcher:quick-chat"),true,{icon_button("attach","attach_file",nil,ui.tr("bead.panel.chat-attach"))},
        {icon_button("terminal","terminal",get("launcher.can_new") and action("launcher:new") or nil,ui.tr("bead.panel.launcher-terminal"))})
    local search_top=compose_top+COMPOSE_H+GAP
    add(kit.TextInput{id="search",position="absolute",left=E,top=search_top,width=INNER,height=BTN,
        text=ui.bind("launcher.search"),placeholder=ui.tr("bead.panel.launcher-search"),padding=T.spacing.sm,padding_left=T.spacing.lg,padding_right=BTN,
        radius=T.radii.full,font_size=CAPTION,fill="surfaceContainerHigh",focus_ring=true})
    local refresh_label=ui.tr("bead.panel.chat-refresh")
    add(icon_button("refresh","refresh",action("launcher:refresh"),refresh_label,false,E+INNER-BTN,search_top))
    local warning=get("launcher.warning")
    local rows_top=search_top+BTN+T.spacing.sm
    local warning_h=warning~="" and META_H+T.spacing.xs or 0
    if warning_h>0 then add(ui.Text{id="launcher-warning",position="absolute",left=E,top=rows_top,width=INNER,height=warning_h,text=warning,font_size=CAPTION,color="onSurfaceVariant"}) end
    rows_top+=warning_h
    local footer_top=H-E-BTN
    local list_h=math.max(0,footer_top-rows_top-GAP)
    local PAD_Y,PAD_X=T.spacing.sm+T.spacing.hairline/2,T.spacing.compact
    local ROW=TITLE_H+META_H+2*PAD_Y
    local GROUP_H=META_H+T.spacing.sm+T.spacing.hairline
    local DOT=T.spacing.sm
    local HALO=DOT+2*(T.spacing.xs-T.spacing.hairline/2)
    local chip_pad=T.spacing.xs+T.spacing.hairline
    add(launcher{id="recent-chats",scroll=ui.bind("launcher.scroll"),position="absolute",left=E,top=rows_top,width=INNER,height=list_h,
        gap=T.spacing.xs,row_height=function(item) return ROW+((item.group or "")~="" and GROUP_H or 0) end,model=ui.bind("launcher.chats"),key="key",render=function(item)
            local active=item.state=="running"
            local pulse=active and ui.animate("chat.running-pulse:"..item.key,get("launcher.pulse") or 0,.2,"out-cubic") or 0
            local prov=string.lower(item.provider or "")
            local prov_w=ui.text_width(prov,T.fonts.mono,MONO)+2*chip_pad+T.spacing.hairline
            local open_label=ui.tr("bead.panel.launcher-open")
            local open_w=ui.text_width(open_label,T.fonts.sans,CAPTION)+2*T.spacing.xs
            local text_w=INNER-2*PAD_X-HALO-prov_w-2*T.spacing.compact-(active and open_w+T.spacing.compact or 0)
            local row=kit.Button{id="chat-row",width=INNER,height=ROW,padding=PAD_X,padding_top=PAD_Y,padding_bottom=PAD_Y,radius=T.spacing.md,fill="surface",
                hover_fill="surfaceContainerHigh",selected_fill="surfaceContainerHigh",selected=item.key==get("launcher.selected"),aria_label=item.title,
                on_activate=get("launcher.can_open") and action("launcher:open:"..item.key) or nil,
                ui.Row{id="line",width=INNER-2*PAD_X,height=ROW-2*PAD_Y,gap=T.spacing.compact,align="center",
                    ui.Box{id="halo",width=HALO,height=HALO,radius=T.radii.full,fill=active and ui.alpha("success",.25*(.55+.45*pulse)) or "transparent",align="center",justify="center",
                        ui.Box{id="running-dot",width=DOT,height=DOT,radius=T.radii.full,fill=active and ui.alpha("success",.55+.45*pulse) or "outlineVariant"}},
                    ui.Column{id="chat-detail",width=text_w,height=ROW-2*PAD_Y,gap=0,
                        ui.Row{id="title-row",width=text_w,height=TITLE_H,
                            ui.Text{id="title",text=item.title,width=text_w,height=TITLE_H,font_size=BODY,font_weight=500,color="onSurface"}},
                        ui.Row{id="meta",width=text_w,height=META_H,
                            ui.Text{id="details",text=((item.project or "")~="" and item.project.." · " or "")..item.lastActivityLabel,width=text_w,height=META_H,font_size=CAPTION,color="onSurfaceVariant"}}},
                    ui.Box{id="provider-chip",width=prov_w,height=META_H+T.spacing.xs,radius=T.spacing.xs+T.spacing.hairline,border_color="outlineVariant",border_width=1,align="center",justify="center",
                        ui.Text{text=prov,width=prov_w-2*chip_pad,height=META_H,font_size=MONO,family=T.fonts.mono,text_align="center",color="onSurfaceVariant"}},
                    active and ui.Text{id="open",text=open_label,width=open_w,height=META_H,font_size=CAPTION,font_weight=500,text_align="center",color="primary"} or nil}}
            if (item.group or "")=="" then return row end
            return ui.Column{id="group",width=INNER,height=ROW+GROUP_H,gap=0,
                ui.Box{id="heading",width=INNER,height=GROUP_H,padding_left=PAD_X,padding_top=T.spacing.sm,
                    ui.Text{text=item.group,width=INNER-PAD_X,height=META_H,font_size=MONO,family=T.fonts.mono,color="onSurfaceVariant"}},
                row}
        end})
    local empty=get("launcher.empty_state")
    if empty~="" then add(ui.Text{id="launcher-message",position="absolute",left=E,top=rows_top+math.max(0,(list_h-META_H)/2),width=INNER,height=META_H,text=empty,font_size=CAPTION,text_align="center",color="onSurfaceVariant"}) end
    local new_label=ui.tr("bead.panel.launcher-new-in-project")
    add(ui.Row{id="secondary",position="absolute",left=E,top=footer_top,width=INNER,height=BTN,gap=T.spacing.xs,align="center",
        kit.PillButton{id="new-chat",height=BTN,label=new_label,label_width=labels[new_label] and labels[new_label].caption or nil,glyph="add_comment",compact=true,
            on_activate=get("launcher.can_new") and action("launcher:new") or nil,aria_label=new_label},
        ui.Box{id="spacer",width=0,grow=1,height=1},
        icon_button("brain","neurology",action("launcher:brain"),ui.tr("bead.panel.launcher-brain")),
        icon_button("settings","settings",action("launcher:settings"),ui.tr("bead.panel.chat-tab-settings"))})
    -- Icon-only footer buttons say their name one hover away.
    local tooltip_height=CAPTION+2*T.spacing.sm
    local function tooltip(id,label,right,bar_size)
        local w=ui.text_width(label,T.fonts.sans,CAPTION)+2*T.spacing.sm
        add(kit.Tooltip{id=id,position="absolute",edge="top",bar_size=bar_size,left=right-w,width=w,height=tooltip_height,padding=T.spacing.xs,visible=true,
            ui.Text{text=label,width="fill",height=META_H,font_size=CAPTION,color="onSurface"}})
    end
    if cued("secondary.brain") then tooltip("footer-tooltip",ui.tr("bead.panel.launcher-brain"),E+INNER-BTN-T.spacing.xs,footer_top-tooltip_height-2*T.spacing.sm) end
    if cued("secondary.settings") then tooltip("footer-tooltip",ui.tr("bead.panel.chat-tab-settings"),E+INNER,footer_top-tooltip_height-2*T.spacing.sm) end
    if cued("refresh") then tooltip("refresh-tooltip",refresh_label,E+INNER,search_top-tooltip_height-2*T.spacing.sm) end
end
-- Motion (design review "Timings for 120 Hz", edge slide): open 240 ms ease-out, close 160 ms
-- (Bead closes with its accelerate curve, at most return_duration). D16: out-quint once the kit has it.
return kit.Panel{id="chat",edge=get("view.edge"),open=ui.bind("panel.open"),duration=.24,return_duration=.16,return_transition="ease",transition="ease",fade=false,reveal=not ui.resolve_for_check(ui.bind("view.detached")),
    width=W,height=H,padding=0,radius=T.radii.medium+T.spacing.hairline,fill="surface",table.unpack(kids)}
