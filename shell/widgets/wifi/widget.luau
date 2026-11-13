-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local status_text=c.status_text
local visual=require("../desktop-visuals")
local function bar(props,settings,_data)
    local glyph=ui.resolve_for_check(ui.bind("bead.desktop.network")) or "wifi_off"
    local active=glyph=="wifi" or glyph=="lan"
    local value=active and "Connected" or "Offline"
    local style=props.form_style
    local content=style=="icon+value" and value or style=="icon+text" and "Wi-Fi" or nil
    local width=content and math.ceil(ui.text_width(content,"IBM Plex Sans",t.detail_font)) or 0
    local box=t.icon_font*t.line_height
    return (popouts and kit.StatusIcon or ui.Row){id="network",direction="row",height=t.status_height,radius=t.status_radius,gap=content and t.item_gap or nil,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
        aria_label=ui.tr("bead.bar.open-network"),on_activate=popouts and ui.action("status.pill.open","network") or nil,
        status_glyph("glyph",ui.bind("bead.desktop.network"),"network",nil,nil,active),
        -- D1: a VPN shows as a small primary lock at the glyph's corner.
        ui.Text{id="vpn",position="absolute",left=t.status_padding+t.icon_width-10,top=(t.status_height-box)/2+box-12,width=12,height=12,visible=ui.bind("bead.desktop.network.vpn"),text="lock",family="Material Symbols Rounded",font_size=10,text_align="center",color="primary"},
        content and status_text{id="value",text=content,width=width,font_size=t.detail_font,color="onSurfaceVariant"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=300,h=150}
    local glyph=ui.resolve_for_check(ui.bind("bead.desktop.network")) or "wifi_off"
    local rows=ui.resolve_for_check(ui.bind("network.rows")) or {}
    local network="No network connected"
    local strength=0
    for _,item in rows do if item.group=="connected" then network=item.label or "Connected"; strength=item.strength or 0; break end end
    local signal=ui.animate("wifi-signal",math.clamp(tonumber(strength) or 0,0,100),.2,"out-cubic")
    local tone=visual.tone(props.instance)
    return ui.Column{id="network-wallpaper",width=slot.w,height=slot.h,fill=tone.scrim or "transparent",radius=tone.scrim and visual.radius or nil,justify="center",gap=kit.tokens.spacing.xs,
        status_glyph("signal-icon",ui.bind("bead.desktop.network"),"network"),
        visual.display("network",network,"fill",48,kit.tokens.text.headline*4/3,tone.ink,tone.shadow),
        visual.shadow_text("strength",tostring(signal).."% signal","fill",22,kit.tokens.text.body*4/3,tone.secondary,tone.shadow or false)}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="wifi",title="Wi-Fi",icon="wifi",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="wallpaper",styles={status={title="Stored status",min_span=1,max_span=1200,preferred_span=300,preferred_height=80,hidden=true,render=function(p,s,d) p.form_style="wallpaper"; return desktop(p,s,d) end},
            glass={title="Network card",min_span=1,max_span=1200,preferred_span=320,preferred_height=154,hidden=true,render=function(p,s,d) p.form_style="glass"; return desktop(p,s,d) end},
            pearl={title="Pearl panel",min_span=1,max_span=1200,preferred_span=320,preferred_height=154,hidden=true,render=function(p,s,d) p.form_style="pearl"; return desktop(p,s,d) end},
            wallpaper={title="Frameless",min_span=1,max_span=1200,preferred_span=360,preferred_height=150,render=desktop}}},
        panel={styles={network={view="network"}}}}
}
