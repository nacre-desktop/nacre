-- SPDX-License-Identifier: GPL-3.0-only
-- Control center (B0049): the QML right-hand sidebar as Luau over kit components. Tiles come from
-- `tiles.luau`; the host projects only data. Sizes are multiples of the shared scale.
local ui=require("bead.ui")
local kit=require("bead.kit")
local reg=require("./tiles")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local popup=get("view.edge")=="top"
local W,PAD,GAP=popup and 410 or reg.layout.width,popup and 15 or T.spacing.xl,popup and 12 or T.spacing.sm
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
local function header(title,back,tools)
    local row={id="header",width=INNER,height=HEADER,gap=HEADER_GAP,align="center"}
    if back then row[#row+1]=tool("back","arrow_back",ui.tr("bead.panel.control-back"),back) end
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
    local slider_w=w-T.spacing.lg-HEAD-T.spacing.sm*2-48-tail_w-T.spacing.md
    local slider_props={id="level",width=slider_w,value=s.value or 0,min=0,max=100,step=5,label=label,enabled=armed,
        on_change=armed and "cc.do" or nil,change_arg=function(n) return "set:"..t.source..":"..n end,
        echo_value=type(s.value_steps)=="number" and s.value_steps>0 and function(n) return math.floor(n/100*s.value_steps+.5)/s.value_steps*100 end or nil,
        pointer_format={prefix="set:"..t.source..":",precision=3},
        local_label=s.status and s.status~="" and {id="../value",format={prefix="",suffix="%",precision=0}} or nil}
    return ui.Row{id="tile-"..t.id,width=w,height=h,radius=T.radii.medium,fill="surfaceContainer",padding_left=T.spacing.lg,padding_right=T.spacing.md,gap=T.spacing.sm,align="center",
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
    local row={id="tile-"..t.id,height=h,padding=inset,gap=between,radius=T.radii.full,fill="surfaceContainer",align="center",shrink=0}
    local w=2*inset+(#modes-1)*between
    for i,m in ipairs(modes) do
        local picked=s.mode==m[1]
        local name=ui.tr(m[3])
        local text_w=textw(name,CAPTION)
        local seg=T.spacing.compact*2+icon+T.spacing.xs+text_w
        w=w+seg
        row[i]=kit.Button{id="mode-"..m[1],width=seg,height=h-2*inset,padding=0,radius=T.radii.full,selected=picked,
            fill=picked and "primaryContainer" or "transparent",hover_fill=picked and "primaryContainer" or ui.alpha("onSurface",.06),aria_label=name,
            on_activate=s.toggle and ui.action("cc.do","choose:"..t.source..":"..m[1]) or nil,
            ui.Row{id="mode-row-"..m[1],width=seg,height=h-2*inset,gap=T.spacing.xs,align="center",justify="center",
                glyph("mode-icon-"..m[1],m[2],picked and "onPrimaryContainer" or "onSurfaceVariant",icon),
                ui.Text{id="mode-label-"..m[1],text=name,width=text_w,height=18,shrink=0,font_size=CAPTION,font_weight=500,
                    color=picked and "onPrimaryContainer" or "onSurface"}}}
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
        return segment(t,h)
    elseif t.kind=="button" then
        return chip("tile-"..t.id,label,t.icon,t.open and ui.action("cc.do","open:"..t.open) or s.press and act(t,"press") or opener,false,
            t.id=="updates" and get("cc.updates") or nil,h)
    elseif t.chip then
        return chip("tile-"..t.id,label,s.icon or t.icon,s.toggle and act(t,"toggle") or opener,s.on==true,nil,h)
    end
    -- A tile: the main press toggles (or opens the page when it cannot); only tiles with a detail page
    -- (Wi-Fi, Bluetooth, Power mode) get the split chevron.
    local split=(t.dock or t.navigate) and opener or nil
    return kit.ToggleTile{id="tile-"..t.id,width=w,height=h,icon=s.icon or t.icon,label=label,detail=s.status,on=s.on==true,
        on_activate=(t.navigate or not s.toggle) and opener or act(t,"toggle"),on_detail=split,
        detail_label=ui.tr("bead.panel.control-details",{name=label})}
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
    local sliders,tiles,chips={},{},{}
    for _,id in ipairs(order) do
        local t=reg.by_id[id]
        if t and supported(t) then
            if t.kind=="slider" then
                local h=reg.layout.sizes.full.height
                table.insert(sliders,{veiled(t,INNER,h,build_tile(t,INNER,h)),h})
            elseif t.kind=="button" or t.kind=="segmented" or t.chip then
                local node,w=build_tile(t,0,CHIP_H)
                table.insert(chips,{veiled(t,w,CHIP_H,node,T.radii.full),w})
            else
                table.insert(tiles,{size="half",build=function(w,h) return veiled(t,w,h,build_tile(t,w,h)) end})
            end
        end
    end
    local items={{ui.Row(status),HEAD}}
    if #sliders==0 and #tiles==0 and #chips==0 then
        table.insert(items,{ui.Text{id="empty",text=ui.tr("bead.panel.control-empty"),width=INNER,height=CAPTION*3,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*3})
    end
    if #sliders>0 then table.insert(items,{stack("sliders",sliders,TILE_GAP)}) end
    if #tiles>0 then
        local grid,height=kit.TileGrid{id="tiles",width=INNER,gap=TILE_GAP,sizes=reg.layout.sizes,tiles=tiles}
        table.insert(items,{grid,height})
    end
    if #chips>0 then table.insert(items,{stack("chips",chip_lines(chips),TILE_GAP)}) end
    table.insert(items,{stack("notices",notices(2,false),TILE_GAP)})
    return stack("home",items,T.spacing.md)
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
    local items={header(ui.tr("bead.panel.control-add"),ui.action("page","edit"))}
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
local function detail()
    local id=get("cc.detail")
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
    local t=reg.by_id[id]
    local s=t and t.source and src[t.source] or {}
    local items={header(t and ui.tr(t.label) or "",ui.action("page","home"))}
    if t and t.page then
        table.insert(items,{ui.Text{id="description",text=ui.tr(t.page),width=INNER,height=BODY*4.5,font_size=BODY,color="onSurfaceVariant"},BODY*4.5})
    end
    if t and s.toggle then
        table.insert(items,{kit.Button{id="switch",width=INNER,height=HEAD+T.spacing.sm,padding=0,radius=T.radii.full,fill=s.on and "surfaceContainerHigh" or "primary",
            aria_label=s.on and ui.tr("bead.panel.control-turn-off") or ui.tr("bead.panel.control-turn-on"),on_activate=ui.action("cc.do","toggle:"..t.source),
            ui.Row{id="switch-row",width=INNER,height=HEAD+T.spacing.sm,align="center",justify="center",
                ui.Text{id="switch-label",text=s.on and ui.tr("bead.panel.control-turn-off") or ui.tr("bead.panel.control-turn-on"),
                    width=INNER-T.spacing.lg*2,height=22,text_align="center",font_size=BODY,color=s.on and "onSurface" or "onPrimary"}}},HEAD+T.spacing.sm})
    end
    for _,c in ipairs(s.choices or {}) do
        table.insert(items,{row_button("choice-"..c.id,t.icon,c.name,c.active and "radio_button_checked" or "radio_button_unchecked",
            ui.action("cc.do","choose:"..t.source..":"..c.id)),ROW})
    end
    if t and s.choices and views[t.view or ""] then
        table.insert(items,{row_button("details","battery_full",ui.tr("bead.panel.control-battery-details"),"chevron_right",ui.action("view.open",t.view)),ROW})
    end
    local note=message()
    if note then table.insert(items,note) end
    return stack("detail",items)
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
-- Owner 2026-11-14: the dock sizes to its content too (it used to keep the full frame height).
local height=math.min(max,content+TOP+PAD)
local inner=body
if content+TOP+PAD>max then
    inner=ui.Scroll{id="scroll",scrollbar=false,scroll=ui.bind("cc.scroll"),width=INNER,height=height-TOP-PAD,body}
end
local edge,open,detached=get("view.edge"),ui.bind("panel.open"),ui.resolve_for_check(ui.bind("view.detached"))
if popup then
    return kit.Panel{id="control-center",edge=edge,open=open,duration=.23,return_duration=.18,return_transition="ease",transition="ease-in-out",fade=false,reveal=not detached,width=W,height=height,padding=PAD,padding_top=TOP,
        gap=GAP,radius=T.radii.medium,fill="surface",inner}
end
-- Owner 2026-11-14 round 3: the dock's surface keeps the frame height (its hover region, so the pointer can
-- travel from the lip into it without closing it) while the visible body sizes to the content.
return kit.Panel{edge=edge,open=open,duration=.28,return_duration=.18,return_transition="ease",transition="ease",fade=false,reveal=not detached,slide_distance=W,width=W,height=max,padding=0,fill="transparent",shadow=false,body="control-center",
    ui.Column{id="control-center",position="absolute",left=0,top=0,width=W,height=height,padding=PAD,padding_top=TOP,gap=GAP,radius=T.radii.medium,fill="surface",inner}}
