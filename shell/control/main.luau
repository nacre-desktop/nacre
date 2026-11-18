-- SPDX-License-Identifier: GPL-3.0-only
-- Control center (B0049): the QML right-hand sidebar as Luau over kit components. Tiles come from
-- `tiles.luau`; the host projects only data. Sizes are multiples of the shared scale.
local ui=require("bead.ui")
local kit=require("bead.kit")
local reg=require("./tiles")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local popup=get("view.edge")=="top"
-- The no-border control centre is a 480 px sheet like the AI sidebar's card: the host says so and gives its width.
local sheet=not popup and get("cc.sheet")
local W,PAD,GAP=popup and 410 or (sheet and get("view.width") or reg.layout.width),popup and 15 or T.spacing.xl,popup and 12 or T.spacing.sm
local INNER=W-2*PAD
local BODY,CAPTION=T.text.body*4/3,T.text.caption*4/3
local HEAD=T.spacing.huge
-- QML NacreControlSurface: content starts 14 px down, the status row is 32 tall with 3 px between its parts, a
-- page header 36 tall, and the page 18 px under the status row (14 under a header). Tool buttons are 32 px circles.
local TOP,HEADER,STATUS_GAP,HEADER_GAP=popup and 15 or T.spacing.lg,T.spacing.huge+T.spacing.xs,3,5
local ROW=T.spacing.huge+T.spacing.md
local page=get("cc.page")
local src,views={},{}
for _,s in ipairs(get("cc.src")) do src[s.id]=s end
for _,v in ipairs(get("cc.views")) do views[v.id]=true end
local function usable(t)
    if t.source then
        local s=src[t.source]
        return s~=nil and s.available==true and (not t.need or s[t.need]==true)
    end
    if t.view then return views[t.view]==true end
    return t.page~=nil or t.open~=nil
end
local function supported(t)
    local s=t.source and src[t.source]
    return not t.hide_unsupported or (s~=nil and s.available==true)
end
local defaults={}
for _,t in ipairs(reg.tiles) do if t.default then table.insert(defaults,t.id) end end
local order
local function ids(list) local out={} for i,item in ipairs(list) do out[i]=item.id end return out end
if get("cc.edited") then order=ids(get("cc.order")) elseif get("cc.saved_known") then order=ids(get("cc.saved")) else order=defaults end
local function present(list)
    local out={}
    for _,id in ipairs(list) do out[id]=true end
    return out
end
local function glyph(id,name,color,size)
    local px=size or T.icons.large*4/3
    return ui.Text{id=id,text=name,family="Material Symbols Rounded",font_size=px,width=px+2,height=24,shrink=0,color=color or "onSurface"}
end
local function stack(id,items,spacing)
    spacing=spacing or GAP
    local kids={id=id,width=INNER,gap=spacing,shrink=0}
    local total=0
    for i,item in ipairs(items) do kids[#kids+1]=item[1] total=total+item[2]+(i>1 and spacing or 0) end
    kids.height=total
    return ui.Column(kids),total
end
-- Width of `text` at `size` px from per-class advances (the layout cannot measure text): capitals and digits .7 em,
-- narrow letters .3, m and w .85, the rest .55, plus 2 px.
local function textw(text,size)
    local em=0
    for _,cp in utf8.codes(text) do
        local c=utf8.char(cp)
        if c=="%" then em=em+.9
        elseif string.find(c,"[A-Z0-9]") then em=em+.7
        elseif string.find(c,"[iljtfr%s%.:,]") then em=em+.3
        elseif c=="m" or c=="w" then em=em+.85
        else em=em+.55 end
    end
    return math.ceil(em*size)+2
end
-- A QML `NacreControlButton`: a 32 px circle, glyph at 17 pt, no fill until hovered; `selected` fills it. It takes no
-- focus ring (QML shows none for a page opened without the keyboard).
local function tool(id,icon,label,action,selected)
    return kit.IconButton{id=id,text=icon,aria_label=label,width=HEAD,height=HEAD,padding=0,radius=T.radii.full,font_size=17*4/3,
        fill=selected and "primaryContainer" or "transparent",color=selected and "onPrimaryContainer" or "onSurface",
        hover_fill="surfaceContainerHighest",focus_ring=false,on_activate=action}
end
local function header(title,back,tools,arrow)
    local row={id="header",width=INNER,height=HEADER,gap=HEADER_GAP,align="center"}
    if back and (arrow or not sheet) then row[#row+1]=tool("back","arrow_back",ui.tr("bead.panel.control-back"),back)
    elseif back then
        -- The "‹ Control centre" chip: returns to the overview (or, on the catalog, to the editor).
        local label=ui.tr("bead.panel.control-overview")
        local chip_w=textw(label,CAPTION)+T.spacing.xl+T.spacing.compact
        row[#row+1]=kit.Button{id="back",width=chip_w,height=30,padding=0,radius=15,shrink=0,fill=ui.alpha("onSurface",.07),hover_fill=ui.alpha("onSurface",.12),
            focus_ring=false,aria_label=ui.tr("bead.panel.control-back"),on_activate=back,
            ui.Row{id="back-row",width=chip_w,height=30,gap=T.spacing.xs,align="center",justify="center",
                glyph("back-icon","chevron_left","onSurfaceVariant",14*4/3),
                ui.Text{id="back-label",text=label,width=chip_w-T.spacing.xl-T.spacing.xs,height=18,shrink=0,font_size=CAPTION,font_weight=500,color="onSurfaceVariant"}}}
    end
    row[#row+1]=ui.Text{id="title",text=title,height=HEAD,grow=1,shrink=1,font_size=T.text.headline*4/3,font_weight=500,color="onSurface"}
    for _,item in ipairs(tools or {}) do row[#row+1]=item end
    return {ui.Row(row),HEADER}
end
local function message()
    local text=get("cc.message")
    if text=="" and get("cc.writable") then return nil end
    return {ui.Text{id="message",text=text~="" and text or ui.tr("bead.panel.control-readonly"),width=INNER,height=CAPTION*3,font_size=CAPTION,
        color=text~="" and "error" or "onSurfaceVariant"},CAPTION*3}
end
-- Design review D4 (N0238) geometry: 56 px slider rows with a 28 px end button, 64 px tiles two per row,
-- 36 px chips that wrap, and 72 px notification rows. Sections carry stable ids (sliders, tiles, chips,
-- notices) so a later unit (D5) can scroll the grown peek to them.
local TILE_GAP,END,CHIP_H=T.spacing.compact,28,36
local function act(t,verb) return ui.action("cc.do",verb..":"..t.source) end
-- A 28 px circle at the end of a slider row (the output device on volume, mic mute on brightness).
local function end_button(id,icon,label,action,ink)
    return kit.IconButton{id=id,text=icon,aria_label=label,width=END,height=END,padding=0,radius=T.radii.full,shrink=0,
        font_size=T.icons.medium*4/3,fill="surface",hover_fill="surfaceContainerHighest",color=ink or "onSurfaceVariant",on_activate=action}
end
-- Level row: icon button (mute) at the left, the stepped slider, the value 38 px wide, then `tail` (an end button).
local function slider_row(t,w,h,tail)
    local s=t.source and src[t.source] or {}
    local label=ui.tr(t.label)
    local armed=s.set==true
    local tail_w=tail and END+T.spacing.sm or 0
    local slider_w=w-4-HEAD-T.spacing.sm*2-48-tail_w
    local slider_props={id="level",width=slider_w,value=s.value or 0,min=0,max=100,step=5,label=label,enabled=armed,
        on_change=armed and "cc.do" or nil,change_arg=function(n) return "set:"..t.source..":"..n end,
        echo_value=type(s.value_steps)=="number" and s.value_steps>0 and function(n) return math.floor(n/100*s.value_steps+.5)/s.value_steps*100 end or nil,
        pointer_format={prefix="set:"..t.source..":",precision=3},
        local_label=s.status and s.status~="" and {id="../value",format={prefix="",suffix="%",precision=0}} or nil}
    return ui.Row{id="tile-"..t.id,width=w,height=h,padding_left=2,padding_right=2,gap=T.spacing.sm,align="center",
        kit.IconButton{id="toggle",text=s.icon or t.icon,aria_label=label,width=HEAD,height=HEAD,padding=0,radius=T.radii.full,shrink=0,font_size=17*4/3,
            fill="transparent",hover_fill=ui.alpha("onSurface",.08),color="primary",on_activate=s.toggle and act(t,"toggle") or nil},
        kit.Slider(slider_props),
        ui.Text{id="value",text=kit.SliderValue(slider_props,s.status or ""),width=48,height=20,shrink=0,text_align="right",font_size=CAPTION,color="onSurfaceVariant"},
        tail}
end
-- The appearance segment: Light | Dark | Auto in one pill (QML NacreControlAppearance), 4 px inside, 2 px between.
local function segment(t,h)
    local s=src[t.source] or {}
    local modes={{"light","light_mode","bead.panel.control-mode-light"},{"dark","dark_mode","bead.panel.control-mode-dark"},
        {"auto","brightness_auto","bead.panel.control-mode-auto"}}
    local inset,between,icon=T.spacing.xs,T.spacing.hairline,T.icons.medium*4/3
    local row={id="tile-"..t.id,height=h,padding=inset,gap=between,radius=T.radii.full,fill=ui.alpha("onSurface",.045),align="center",shrink=0}
    local w=2*inset+(#modes-1)*between
    for i,m in ipairs(modes) do
        local picked=s.mode==m[1]
        local name=ui.tr(m[3])
        local text_w=textw(name,CAPTION)
        local seg=T.spacing.compact*2+icon+T.spacing.xs+text_w
        w=w+seg
        row[i]=kit.Button{id="mode-"..m[1],width=seg,height=h-2*inset,padding=0,radius=T.radii.full,selected=picked,
            fill=picked and ui.alpha("onSurface",.12) or "transparent",hover_fill=picked and ui.alpha("onSurface",.12) or ui.alpha("onSurface",.06),aria_label=name,
            on_activate=s.toggle and ui.action("cc.do","choose:"..t.source..":"..m[1]) or nil,
            ui.Row{id="mode-row-"..m[1],width=seg,height=h-2*inset,gap=T.spacing.xs,align="center",justify="center",
                glyph("mode-icon-"..m[1],m[2],picked and "onSurface" or "onSurfaceVariant",icon),
                ui.Text{id="mode-label-"..m[1],text=name,width=text_w,height=18,shrink=0,font_size=CAPTION,font_weight=500,
                    color=picked and "onSurface" or "onSurfaceVariant"}}}
    end
    row.width=w
    return ui.Row(row),w
end
-- A chip: icon, label and an optional count badge in a pill; `on` fills it with the primary container.
local function chip(id,label,icon,action,on,count,h)
    h=h or CHIP_H
    local armed=action~=nil
    local ink,icon_ink=on and "onPrimaryContainer" or "onSurface",on and "onPrimaryContainer" or "onSurfaceVariant"
    local icon_px=T.icons.medium*4/3
    -- 500-weight labels run a little wider than the estimate ("Wallpaper")
    local text_w=textw(label,CAPTION)+T.spacing.xs
    local w=T.spacing.compact+icon_px+T.spacing.xs+text_w+T.spacing.lg
    local badge
    if count and count>0 then
        local text=tostring(count)
        local bw=math.max(15,textw(text,T.text.caption)+4)
        badge=ui.Box{id="badge",position="absolute",left=T.spacing.compact+icon_px-bw+6,top=(h-icon_px)/2-5,width=bw,height=15,radius=T.radii.full,fill="primary",
            ui.Text{id="count",text=text,width=bw,height=15,text_align="center",font_size=T.text.caption,font_weight=600,color="onPrimary"}}
    end
    return kit.Button{id=id,width=w,height=h,padding=0,shrink=0,radius=T.radii.full,selected=on,
        fill=ui.alpha(on and "primaryContainer" or "surfaceContainer",armed and 1 or .4),hover_fill=on and "primaryContainer" or "surfaceContainerHigh",
        aria_label=label,on_activate=action,
        ui.Row{id=id.."-row",width=w,height=h,padding_left=T.spacing.compact,gap=T.spacing.xs,align="center",
            glyph(id.."-icon",icon,icon_ink,icon_px),
            ui.Text{id=id.."-label",text=label,width=text_w,height=18,shrink=0,font_size=CAPTION,font_weight=500,color=ink}},
        badge},w
end
-- A section row on a hairline: icon, name over detail, then a switch, a chevron and a count badge as given. Only the
-- row's own button reacts to the pointer; nothing is boxed, and the hairline is part of the row's height (h + 1).
local RULE=ui.alpha("onSurface",.07)
local function sect_row(p,w,h)
    local trail,n=0,1
    for _,part in ipairs({p.switch and 46 or false,p.badge and 22 or false,p.go and HEAD or false}) do
        if part then trail=trail+part n=n+1 end
    end
    local gap=T.spacing.xs
    local main_w=w-trail-(n-1)*gap
    local icon_px=T.icons.large*4/3
    local text_w=main_w-2-icon_px-2-T.spacing.md
    local lines={ui.Text{id="label",text=p.label,width=text_w,height=22,font_size=BODY,font_weight=500,color="onSurface"}}
    if p.detail and p.detail~="" then lines[2]=ui.Text{id="detail",text=p.detail,width=text_w,height=18,font_size=CAPTION,color="onSurfaceVariant"} end
    local row={id="row",width=w,height=h,gap=gap,align="center",
        kit.Button{id="main",width=main_w,height=h,padding=0,padding_left=2,radius=T.radii.small,fill="transparent",hover_fill=ui.alpha("onSurface",.05),
            aria_label=p.label,on_activate=p.main,
            ui.Row{id="main-row",width=main_w,height=h,padding_left=2,gap=T.spacing.md,align="center",
                glyph("icon",p.icon,p.on and "primary" or "onSurfaceVariant"),
                ui.Column(table.move(lines,1,#lines,1,{id="text",width=text_w,height=#lines*20,shrink=0}))}}}
    if p.switch then row[#row+1]=kit.Switch{id="switch",checked=p.on==true,aria_label=p.label,on_activate=p.toggle} end
    if p.badge then
        local text=tostring(p.badge)
        row[#row+1]=ui.Box{id="badge",width=22,height=18,radius=T.radii.full,fill="error",align="center",justify="center",shrink=0,
            ui.Text{id="count",text=text,width=22,height=18,text_align="center",font_size=T.text.caption,font_weight=600,color="onError"}}
    end
    if p.go then
        row[#row+1]=kit.IconButton{id="detail",text="chevron_right",aria_label=p.go_label or p.label,width=HEAD,height=HEAD,padding=0,radius=T.radii.full,shrink=0,
            font_size=17*4/3,fill="transparent",hover_fill=ui.alpha("onSurface",.08),color="onSurfaceVariant",on_activate=p.go}
    end
    return ui.Column{id=p.id,width=w,height=h+1,gap=0,shrink=0,ui.Row(row),ui.Box{id="rule",width=w,height=1,shrink=0,fill=RULE}}
end
local function build_tile(t,w,h,embedded)
    local s=t.source and src[t.source] or {}
    local label=ui.tr(t.label)
    local opener
    if t.dock and not embedded then opener=ui.action("detail",t.dock)
    elseif t.view and views[t.view] then opener=ui.action("view.open",t.view)
    elseif t.page then opener=ui.action("detail",t.id) end
    if t.kind=="slider" then
        local tail
        if t.id=="volume" and not embedded then
            tail=end_button("device","speaker",ui.tr("bead.panel.control-output-device"),opener)
        elseif t.id=="brightness" and not embedded then
            local mic=src["audio.input"]
            if mic and mic.available then
                -- the mic is an input: its mute lives at the end of the brightness row, red while muted
                tail=end_button("mic",mic.on and "mic_off" or "mic",mic.on and ui.tr("bead.panel.control-mic-unmute") or ui.tr("bead.panel.control-mic"),
                    mic.toggle and ui.action("cc.do","toggle:audio.input") or nil,mic.on and "error" or nil)
            end
        end
        return slider_row(t,w,h,tail)
    elseif t.kind=="segmented" then
        local seg=segment(t,h-T.spacing.md)
        return ui.Column{id="row-"..t.id,width=w,height=h+1,gap=0,shrink=0,
            ui.Row{id="row",width=w,height=h,padding_left=2,gap=T.spacing.md,align="center",
                glyph("icon",t.icon,"onSurfaceVariant"),
                ui.Text{id="label",text=label,width=0,grow=1,height=22,font_size=BODY,font_weight=500,color="onSurface"},seg},
            ui.Box{id="rule",width=w,height=1,shrink=0,fill=RULE}}
    elseif t.kind=="button" then
        local action=t.open and ui.action("cc.do","open:"..t.open) or s.press and act(t,"press") or opener
        return sect_row({id="tile-"..t.id,icon=t.icon,label=label,main=action,go=action,go_label=label,
            badge=t.id=="updates" and get("cc.updates")>0 and get("cc.updates") or nil},w,h)
    end
    -- A toggle: the row's main press toggles (or opens the page when it cannot); a switch shows the state, and only
    -- rows with a detail page (Wi-Fi, Bluetooth, VPN, Power mode) get the chevron.
    local split=(t.dock or t.navigate) and opener or nil
    local toggles=s.toggle and not t.navigate
    return sect_row({id="tile-"..t.id,icon=s.icon or t.icon,label=label,detail=s.status,on=s.on==true,
        main=(t.navigate or not s.toggle) and opener or act(t,"toggle"),switch=toggles or nil,toggle=toggles and act(t,"toggle") or nil,
        go=split,go_label=ui.tr("bead.panel.control-details",{name=label})},w,h)
end
local notice=require("../ui/components/notice-row")
local NOTICE_H=notice.height
-- A notification row (shared with the dashboard's Notifications page); × dismisses (history rows are forgotten).
local function notice_row(n)
    return notice.Row(n,{width=INNER,on_dismiss=n.rev~=nil and ui.action("cc.do","dismiss:notifications:"..n.id.."|"..n.rev) or nil})
end
local function notices_empty()
    local text=get("cc.notice_ready") and ui.tr("bead.panel.control-notices-empty") or ui.tr("bead.panel.control-notices-loading")
    local height=popup and 22 or CAPTION*2
    return {ui.Text{id="notices-empty",text=text,width=INNER,height=height,font_size=popup and BODY or CAPTION,color="onSurfaceVariant"},height}
end
-- The notification section under the chips: the title with the DND and Clear chips, then the newest entries;
-- limit 0 supplies only the heading and status text for the windowed popup list.
local function notices(limit,page)
    local list=get("cc.notices")
    local clear=get("cc.notice_clear")
    local dnd=src["notifications.dnd"]
    local head_h=CHIP_H-6
    local row={id="notices-head",width=INNER,height=head_h,gap=T.spacing.sm,align="center"}
    local title=ui.tr("bead.panel.control-notifications")
    -- the heading opens the full in-panel list (DND lost its chevron: it is a plain toggle)
    row[#row+1]=page and ui.Text{id="notices-title",text="",height=head_h,grow=1,shrink=1,font_size=BODY,color="onSurface"}
        or kit.Button{id="notices-title",height=head_h,grow=1,shrink=1,padding=0,radius=T.radii.small,fill="transparent",hover_fill=ui.alpha("onSurface",.06),
            aria_label=title,on_activate=ui.action("detail","notifications"),
            ui.Text{id="label",text=title,width="fill",height=head_h,font_size=BODY,font_weight=600,color="onSurface"}}
    local on=dnd~=nil and dnd.on==true
    row[#row+1]=chip("notices-dnd",ui.tr("bead.panel.control-pill-dnd"),on and "do_not_disturb_on" or "notifications_active",
        dnd and dnd.toggle and ui.action("cc.do","toggle:notifications.dnd") or nil,on,nil,head_h)
    row[#row+1]=chip("notices-clear",ui.tr("bead.panel.control-pill-clear"),"clear_all",clear and ui.action("cc.do","clear:notifications") or nil,false,nil,head_h)
    local items={{ui.Row(row),head_h}}
    local note=get("cc.notice_note")
    if note and note~="" then
        table.insert(items,{kit.Paragraph{id="notices-window",text=note,width=INNER,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*2})
    end
    if #list==0 then table.insert(items,notices_empty()) end
    for i,n in ipairs(list) do
        if not limit or i<=limit then table.insert(items,{notice_row(n),NOTICE_H}) end
    end
    return items
end
-- Chips wrap into lines by their measured widths.
local function chip_lines(nodes)
    local lines,line,used={},nil,0
    for _,c in ipairs(nodes) do
        if not line or used+TILE_GAP+c[2]>INNER then line={} table.insert(lines,line) used=c[2] else used=used+TILE_GAP+c[2] end
        table.insert(line,c[1])
    end
    local rows={}
    for i,l in ipairs(lines) do rows[i]={ui.Row(table.move(l,1,#l,1,{id="line-"..i,width=INNER,height=CHIP_H,gap=TILE_GAP})),CHIP_H} end
    return rows
end
local function home()
    local status_w=INNER-3*HEAD-3*STATUS_GAP
    local estimate=get("cc.battery_estimate")
    -- QML: glyph 15 pt, percentage 10 pt medium, estimate 9 pt muted, 5 px apart
    local percent=get("cc.battery")
    local pct,note=T.text.caption*4/3,(T.text.caption-1)*4/3
    local pct_w=textw(percent,pct)
    local status={id="status",width=INNER,height=HEAD,gap=STATUS_GAP,align="center",
        kit.Button{id="battery-status",width=status_w,height=HEAD,padding=0,fill="transparent",hover_fill="transparent",
            aria_label=ui.tr("bead.panel.control-battery-details"),on_activate=views.battery and ui.action("detail","battery") or nil,
            ui.Row{id="battery-row",width=status_w,height=HEAD,gap=5,align="center",
                glyph("battery-glyph",get("cc.battery_icon"),"primary",T.icons.medium*4/3),
                ui.Text{id="battery-text",text=percent,width=estimate=="" and status_w-27 or pct_w,height=24,font_size=pct,font_weight=500,color="onSurface"},
                estimate~="" and ui.Text{id="battery-estimate",text=estimate,width=status_w-27-pct_w-5,height=24,font_size=note,color="onSurfaceVariant"} or nil}},
        tool("edit","edit",ui.tr("bead.panel.control-edit"),ui.action("begin",table.concat(order,",")),page=="edit"),
        tool("settings","settings",ui.tr("bead.panel.control-settings"),ui.action("cc.do","open:settings")),
        tool("power","power_settings_new",ui.tr("bead.panel.control-power"),views.session and ui.action("view.open","session") or nil,false)}
    -- Supported but temporarily unavailable controls keep their place under a veil.
    local function veiled(t,w,h,node,radius)
        if usable(t) then return node end
        return ui.Box{id="dim-"..t.id,width=w,height=h,node,
            ui.Box{id="veil",position="absolute",left=0,top=0,width=w,height=h,radius=radius or T.radii.medium,fill=ui.alpha("surface",0.6)}}
    end
    -- Four sections (spacing and hairlines, no boxes): each tile belongs to the group the registry names, in the
    -- user's order. Wallpaper's two actions share one row of buttons that links to Strand.
    local sections={conn={},sound={},wall={},system={}}
    local wall_chips
    for _,id in ipairs(order) do
        local t=reg.by_id[id]
        if t and supported(t) then
            local list=sections[t.group or "system"]
            if t.group=="wall" then
                wall_chips=wall_chips or {}
                local node,w=chip("tile-"..t.id,ui.tr(t.label),t.icon,t.open and ui.action("cc.do","open:"..t.open) or src[t.source] and src[t.source].press and act(t,"press") or nil,false,nil,CHIP_H)
                table.insert(wall_chips,usable(t) and node or ui.Box{id="dim-"..t.id,width=w,height=CHIP_H,node,
                    ui.Box{id="veil",position="absolute",left=0,top=0,width=w,height=CHIP_H,radius=T.radii.full,fill=ui.alpha("surface",0.6)}})
            elseif t.kind=="slider" then
                table.insert(list,{ui.Column{id="row-"..t.id,width=INNER,height=ROW+1,gap=0,shrink=0,veiled(t,INNER,ROW,build_tile(t,INNER,ROW)),
                    ui.Box{id="rule",width=INNER,height=1,shrink=0,fill=RULE}},ROW+1})
            elseif t.kind=="segmented" or t.kind=="button" or t.kind=="toggle" then
                table.insert(list,{veiled(t,INNER,ROW+1,build_tile(t,INNER,ROW),0),ROW+1})
            end
        end
    end
    local items={{ui.Row(status),HEAD}}
    local any=false
    for _,g in ipairs({{"conn","bead.panel.control-group-connectivity"},{"sound","bead.panel.control-group-sound"},
        {"wall","bead.panel.control-group-wallpaper"},{"system","bead.panel.control-group-system"}}) do
        local list=sections[g[1]]
        if g[1]=="wall" and wall_chips then
            local row_h=CHIP_H+T.spacing.sm
            list={{ui.Row(table.move(wall_chips,1,#wall_chips,1,{id="wall-actions",width=INNER,height=row_h,gap=T.spacing.sm,align="center"})),row_h}}
        end
        if #list>0 then
            any=true
            local pad=#items==1 and T.spacing.md or T.spacing.xl
            table.insert(items,{ui.Box{id="group-"..g[1],width=INNER,height=pad+22,padding_top=pad,shrink=0,
                ui.Text{id="title",text=ui.tr(g[2]),width=INNER,height=22,font_size=CAPTION,font_weight=600,color="onSurfaceVariant"}},pad+22})
            for _,item in ipairs(list) do table.insert(items,item) end
        end
    end
    if not any then
        table.insert(items,{ui.Text{id="empty",text=ui.tr("bead.panel.control-empty"),width=INNER,height=CAPTION*3,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*3})
    end
    table.insert(items,{ui.Box{id="notices-gap",width=INNER,height=T.spacing.lg,shrink=0},T.spacing.lg})
    table.insert(items,{stack("notices",notices(2,false),TILE_GAP)})
    return stack("home",items,0)
end
local function row_button(id,icon,label,detail,action,sub)
    local ink=sub and "onSurfaceVariant" or "onSurface"
    local text_w=INNER-ROW-T.spacing.lg*2
    local lines={ui.Text{id=id.."-label",text=label,width=text_w,height=sub and 20 or 22,font_size=sub and CAPTION or BODY,color=ink}}
    if sub then lines[2]=ui.Text{id=id.."-sub",text=sub,width=text_w,height=18,font_size=CAPTION,color="onSurfaceVariant"} end
    return kit.Button{id=id,width=INNER,height=ROW,padding=0,radius=T.radii.medium,on_activate=action,aria_label=label,
        ui.Row{id=id.."-row",width=INNER,height=ROW,padding_left=T.spacing.md,gap=T.spacing.md,align="center",
            glyph(id.."-icon",icon,ink),ui.Column(table.move(lines,1,#lines,1,{id=id.."-text",width=text_w})),
            glyph(id.."-detail",detail,ink)}}
end
local function edit()
    local visible={}
    for _,id in ipairs(order) do
        local t=reg.by_id[id]
        if t and supported(t) then table.insert(visible,{id=id,label=ui.tr(t.label),icon=t.icon}) end
    end
    for _,item in ipairs(visible) do
        item.build=function(w,h)
            return ui.Row{id="item",width=w,height=h,padding_left=T.spacing.md,gap=T.spacing.md,align="center",
                glyph("handle","drag_indicator","onSurfaceVariant"),glyph("icon",item.icon,"primary"),
                ui.Text{id="label",text=item.label,width=w-h-T.spacing.md*2-2*(T.icons.large*4/3+2)-T.spacing.md*2,height=22,font_size=BODY,color="onSurface"}}
        end
    end
    local items={header(ui.tr("bead.panel.control-edit-title"),ui.action("page","home"),{
        kit.Button{id="reset",width=HEAD*2,height=HEAD,padding=0,radius=T.radii.full,fill="surfaceContainer",
            aria_label=ui.tr("bead.panel.control-reset"),on_activate=ui.action("reset",table.concat(defaults,",")),
            ui.Row{id="reset-row",width=HEAD*2,height=HEAD,align="center",justify="center",
                ui.Text{id="reset-label",text=ui.tr("bead.panel.control-reset"),width=HEAD*2-T.spacing.md,height=22,text_align="center",font_size=BODY,color="onSurface"}}}}),
        {ui.Text{id="hint",text=ui.tr("bead.panel.control-edit-hint"),width=INNER,height=CAPTION*3,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*3}}
    if #visible>0 then
        table.insert(items,{kit.ReorderList{id="order",width=INNER,item_height=ROW,gap=GAP,items=visible,on_touch="touch",
            remove=function(item,h)
                return kit.IconButton{id="remove",text="close",aria_label=ui.tr("bead.panel.control-remove",{name=item.label}),width=h,height=h,
                    on_activate=ui.action("remove",item.id)}
            end},#visible*(ROW+GAP)-GAP})
    end
    table.insert(items,{row_button("add","add",ui.tr("bead.panel.control-add"),"chevron_right",ui.action("page","catalog")),ROW})
    local note=message()
    if note then table.insert(items,note) end
    return stack("edit",items)
end
local function catalog()
    local inside=present(order)
    local ready,waiting={},{}
    for _,t in ipairs(reg.tiles) do
        if not inside[t.id] then table.insert(usable(t) and ready or waiting,t) end
    end
    local items={header(ui.tr("bead.panel.control-add"),ui.action("page","edit"),nil,true)}
    for _,t in ipairs(ready) do
        local label=ui.tr(t.label)
        table.insert(items,{row_button("add-"..t.id,t.icon,label,"add",ui.action("add",t.id)),ROW})
    end
    if #waiting>0 then
        table.insert(items,{ui.Text{id="waiting",text=ui.tr("bead.panel.control-waiting"),width=INNER,height=24,font_size=BODY,color="onSurfaceVariant"},24})
        for _,t in ipairs(waiting) do
            table.insert(items,{row_button("wait-"..t.id,t.icon,ui.tr(t.label),"hourglass_empty",nil,ui.tr("bead.panel.control-needs",{contract=t.ask or t.id})),ROW})
        end
    end
    return stack("catalog",items)
end
local function notifications_page()
    local group,group_height=stack("notices",notices(0,true),T.spacing.md)
    local prefix,prefix_height=stack("detail",{
        header(ui.tr("bead.panel.control-notifications"),ui.action("page","home")),
        {group,group_height}},T.spacing.lg)
    local height=math.max(1,get("cc.max_height")-TOP-PAD)
    local list=require("bead.widgets.kit.virtual-list"){id="history-scroll",scroll=ui.bind("cc.scroll"),
        width=INNER,height=height,model=ui.bind("cc.notices"),key="id",row_height=NOTICE_H,gap=T.spacing.md,
        before=prefix,before_height=prefix_height,before_gap=#get("cc.notices")>0 and T.spacing.md or 0,render=notice_row}
    return list,height
end
-- The power button's page: the session menu's actions (inline confirmation for restart, log out and power
-- off, like the power dropdown) and the power mode tile.
local function session_page()
    local enabled=get("session.enabled") and not get("session.busy")
    local question,asking=get("session.confirm"),get("session.confirm_action")
    local entries={{"lock","bead.panel.session-lock","lock"},{"logout","bead.panel.session-logout","logout"},
        {"suspend","bead.panel.session-suspend","bedtime"},{"reboot","bead.panel.session-reboot","restart_alt"},
        {"shutdown","bead.panel.session-shutdown","power_settings_new"}}
    local tile_h=T.spacing.huge+T.spacing.sm
    local items={header(ui.tr("bead.panel.control-power-title"),ui.action("page","home"))}
    for _,entry in ipairs(entries) do
        local key,label=entry[1],ui.tr(entry[2])
        table.insert(items,{kit.ToggleTile{id="act-"..key,width=INNER,height=tile_h,icon=entry[3],label=label,on=asking==key,
            available=enabled or asking==key,aria_label=label,
            on_activate=enabled and (asking==key and ui.action("session.confirm") or ui.action("session.select",key)) or nil},tile_h})
    end
    if question~="" and asking~="" then
        local half=(INNER-2*T.spacing.md-T.spacing.sm)/2
        local label=question
        for _,entry in ipairs(entries) do if entry[1]==asking then label=ui.tr(entry[2]) end end
        table.insert(items,{kit.Card{id="confirmation",width=INNER,height=104,shrink=0,padding=T.spacing.md,gap=T.spacing.md,radius=T.radii.medium,
            ui.Text{id="question",text=question,width=INNER-2*T.spacing.md,height=28,font_size=T.text.title*4/3,color="onSurface"},
            ui.Row{id="buttons",width=INNER-2*T.spacing.md,height=40,gap=T.spacing.sm,
                kit.Button{id="cancel",width=half,height=40,padding=0,radius=T.radii.full,aria_label=ui.tr("bead.common.cancel"),on_activate=ui.action("session.cancel"),
                    ui.Row{id="cancel-row",width=half,height=40,align="center",justify="center",
                        ui.Text{id="cancel-label",text=ui.tr("bead.common.cancel"),width=half-T.spacing.md,height=22,text_align="center",font_size=BODY,color="onSurface"}}},
                kit.Button{id="confirm",width=half,height=40,padding=0,radius=T.radii.full,fill="primary",aria_label=question,on_activate=enabled and ui.action("session.confirm") or nil,
                    ui.Row{id="confirm-row",width=half,height=40,align="center",justify="center",
                        ui.Text{id="confirm-label",text=label,width=half-T.spacing.md,height=22,text_align="center",font_size=BODY,color="onPrimary"}}}}},104})
    end
    local mode=reg.by_id.battery
    if supported(mode) then table.insert(items,{build_tile(mode,INNER,reg.layout.sizes.half.height),reg.layout.sizes.half.height}) end
    local message=get("session.message")
    if message~="" then table.insert(items,{ui.Text{id="message",text=message,width=INNER,height=CAPTION*3,font_size=CAPTION,color="error"},CAPTION*3}) end
    return stack("detail",items,T.spacing.sm)
end
local function detail()
    local id=get("cc.detail")
    if id=="session" then return session_page() end
    local dock={network={function() return require("../status/network-body")({width=INNER}) end,"bead.panel.control-network"},
        bluetooth={function() return require("../status/bluetooth-body")({width=INNER}) end,"bead.panel.control-bluetooth"},
        battery={function() return require("../status/battery-body")({width=INNER}) end,"bead.panel.control-power-brightness"},
        audio={function() return require("../sound/body")({width=INNER}) end,"bead.panel.control-sound"},
        vpn={function() return require("./vpn-body")({width=INNER}) end,"bead.panel.control-vpn"}}
    if dock[id] then
        local node,height=dock[id][1]()
        local items={header(ui.tr(dock[id][2]),ui.action("page","home")),{node,height}}
        if id=="battery" then
            for _,tile in ipairs({reg.by_id.brightness,reg.by_id.keyboard}) do
                if supported(tile) then
                    table.insert(items,{build_tile(tile,INNER,reg.layout.sizes.full.height,true),reg.layout.sizes.full.height})
                end
            end
        end
        return stack("detail",items,T.spacing.lg)
    end
    if id=="notifications" or id=="dnd" then return notifications_page() end
    if id=="updates" then
        -- The status popout's data (`cc.updates_*`): summary, checked line, health note and the package list.
        local items={header(ui.tr("bead.panel.control-updates"),ui.action("page","home")),
            {ui.Text{id="updates-summary",text=ui.bind("cc.updates_summary"),width=INNER,height=BODY*1.5,font_size=BODY,color="onSurface"},BODY*1.5},
            {ui.Text{id="updates-checked",text=ui.bind("cc.updates_checked"),width=INNER,height=CAPTION*1.5,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*1.5}}
        local health=get("cc.updates_health")
        if health~="" then
            local note=kit.Paragraph{id="updates-health",text=health,width=INNER,font_size=CAPTION,color="onSurfaceVariant"}
            table.insert(items,{note,note.props.height})
        end
        local button_height=T.spacing.xxl+T.spacing.md
        local buttons={ui.Row{id="updates-actions",width=INNER,height=button_height,gap=T.spacing.sm,
            kit.ActionButton{id="updates-now",label=ui.tr("bead.panel.updates-open"),primary=true,
                on_activate=get("cc.updates_can_open") and ui.action("cc.do","open:updates") or nil,height=button_height},
            kit.ActionButton{id="updates-check",label=ui.tr("bead.panel.updates-check"),
                on_activate=get("cc.updates_can_refresh") and ui.action("cc.do","press:updates.refresh") or nil,height=button_height}},button_height}
        local system={row_button("updates-open","system_update",ui.tr("bead.panel.control-updates-open"),"chevron_right",ui.action("view.open","dashboard-system")),ROW}
        local model=ui.bind("cc.updates_packages")
        local count=ui.model_count(model)
        if count>0 then
            local row_h,gap=ROW,T.spacing.xs
            local used=TOP+PAD+(#items+3)*GAP+buttons[2]+system[2]
            for _,item in ipairs(items) do used=used+item[2] end
            local fit=get("cc.max_height")-used
            local list_h=math.min(count*(row_h+gap)-gap,math.max(row_h,fit))
            table.insert(items,{require("bead.widgets.kit.virtual-list"){id="updates-packages",model=model,key="id",scroll=ui.bind("cc.updates_scroll"),
                width=INNER,height=list_h,row_height=row_h,gap=gap,render=function(item)
                    return ui.Column{id="package",width=INNER,height=row_h,padding_left=T.spacing.md,gap=0,justify="center",shrink=0,
                        radius=T.radii.small,fill="surfaceContainer",
                        ui.Text{id="name",text=item.name,width=INNER-T.spacing.md*2,height=22,font_size=BODY,color="onSurface"},
                        ui.Text{id="versions",text=item.old.." → "..item.new,width=INNER-T.spacing.md*2,height=18,font_size=CAPTION,color="onSurfaceVariant"}}
                end},list_h})
        end
        table.insert(items,buttons)
        table.insert(items,system)
        return stack("detail",items)
    end
    -- 0280: an unknown or empty id (a remembered or routed page) is the overview, never a bare back row.
    return home()
end
local body,content
if popup then
    local items=notices(0,false)
    local heading=table.remove(items,1)
    local prefix,prefix_height=stack("history",items,GAP)
    local count=#get("cc.notices")
    local prefix_gap=prefix_height>0 and count>0 and GAP or 0
    local list_height=prefix_height+prefix_gap+math.max(0,count*(NOTICE_H+GAP)-GAP)
    -- Keep DND/Clear visible; construct only the viewport's retained cards.
    local available=math.max(0,math.min(10*ROW,get("cc.max_height")-TOP-PAD-heading[2]-GAP))
    local visible=math.min(available,list_height)
    local list=require("bead.widgets.kit.virtual-list"){id="scroll",scroll=ui.bind("cc.scroll"),
        width=INNER,height=visible,model=ui.bind("cc.notices"),key="id",row_height=NOTICE_H,gap=GAP,
        before=prefix,before_height=prefix_height,before_gap=prefix_gap,render=notice_row}
    body,content=stack("history-content",{heading,{list,visible}},GAP)
elseif page=="edit" then body,content=edit()
elseif page=="catalog" then body,content=catalog()
elseif page=="detail" then body,content=detail()
else body,content=home() end
local max=get("cc.max_height")
-- D16: "ease" is the kit's EaseOut, the closest curve to the review's out-cubic open / in-cubic close until the
-- named easing tokens land (Bead B0263); D5 retargets the peek into this surface.
-- 0280: the dock keeps the frame height on every page. Its body is its hit region, so a body that shrank under
-- the pointer handed it to the click-outside catcher and the hover close fired. Long pages scroll inside.
local height=popup and math.min(max,content+TOP+PAD) or max
local inner,inner_height=body,content
if content+TOP+PAD>max then
    inner_height=height-TOP-PAD
    inner=ui.Scroll{id="scroll",scrollbar=false,scroll=ui.bind("cc.scroll"),width=INNER,height=inner_height,body}
end
if sheet then
    -- A page change (every one alternates `cc.flip`) slides the new page in from the right: 28 px over 220 ms.
    local flip=get("cc.flip")
    local offset=math.floor(28*math.abs(flip-ui.animate("cc.slide",flip,.22,"out-cubic"))+.5)
    if offset>0 then inner=ui.Column{id="slide",width=INNER,height=inner_height,padding_left=offset,shrink=0,inner} end
end
-- The sheet enters with a 16 px slide and a fade (the AI sidebar's card); the duration is the panel profile's, panel views author none.
return kit.Panel{id="control-center",edge=get("view.edge"),open=ui.bind("panel.open"),fade=sheet and true or false,reveal=not sheet and not ui.resolve_for_check(ui.bind("view.detached")),slide_distance=sheet and 16 or (not popup and W or nil),
    width=W,height=height,padding=PAD,padding_top=TOP,
    gap=GAP,radius=T.radii.medium,fill="surface",
    -- Lacquer sheet (locked 2026-11-17): a 1 px rim, a lit top edge and a short contact shadow on the floating card.
    border_width=sheet and 1 or nil,border_color=sheet and ui.alpha("onSurface",.05) or nil,shadow=sheet and {blur=4,spread=0,dy=1,alpha=.6} or nil,
    inner,sheet and ui.Box{id="lit-edge",position="absolute",left=T.radii.medium*.6,right=T.radii.medium*.6,top=0,height=1,fill=ui.alpha("surfaceBright",.6)} or nil}
