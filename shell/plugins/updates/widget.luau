-- SPDX-License-Identifier: GPL-3.0-only
-- First-party plugin (spec 0203): the entry returns a factory over the host API.
return function(api)
local c=api.app.context
local ui=api.ui
local kit=api.kit
local t=c.t
local measure=api.app.measure
local status_text=c.status_text
local popouts=c.popouts
local updates_on=c.updates_on
local status_glyph=c.status_glyph
local visual=api.app.visual
local font="IBM Plex Sans"
local item_width=t.status_padding+t.icon_width+t.status_count_padding
local function bar(props,settings,_data)
    local style=props.form_style
    -- D1: deployed_code_update in the variant tone with a primary count badge (hidden at 0).
    local count=tonumber(ui.resolve_for_check(ui.bind("bar.updates.text")) or "0") or 0
    local badge=style~="icon+text" and count>0 and (count>999 and "999+" or ui.bind("bar.updates.text")) or nil
    local text=style=="icon+text" and measure.ellipsize("Updates",props.available_width and math.max(0,props.available_width-item_width-t.item_gap) or ui.text_width("Updates",font,t.detail_font),font,t.detail_font) or nil
    local width=text and math.ceil(ui.text_width(text,font,t.detail_font)) or 0
    local action=popouts and ui.action("status.pill.open","updates") or nil
    return updates_on and kit.Button{id="updates",width=text and props.available_width or nil,direction="row",padding=0,fill="transparent",hover_fill=ui.alpha("onSurface",t.status_hover),on_activate=action,height=t.status_height,radius=t.status_radius,gap=text and t.item_gap or 0,padding_left=t.status_padding,padding_right=t.status_count_padding,align="center",justify=text and "start" or "center",
        status_glyph("package","deployed_code_update","package",nil,badge,false),
        text and status_text{id="label",text=text,width=width,font_size=t.detail_font,color="onSurfaceVariant"} or nil,
    } or ui.Box{id="updates",width=0,height=0,shrink=0,visible=false}
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=320,h=160}
    local width=slot.w
    local height=slot.h
    local count=ui.resolve_for_check(ui.bind("bar.updates.text")) or ""
    local checked=ui.bind("status.updates_checked")
    local open=ui.resolve_for_check(ui.bind("status.updates_can_open"))==true
    return visual.glass("updates-desktop",width,height,kit.tokens.spacing.lg,{
        ui.Column{id="updates-card",width="fill",height="fill",justify="center",gap=kit.tokens.spacing.sm,
            ui.Text{id="title",text="System updates",width="fill",height=24,font_size=kit.tokens.text.body*4/3,color="onSurfaceVariant"},
            ui.Row{id="count-row",width="fill",height=70,align="center",gap=kit.tokens.spacing.sm,
                visual.display("count",count,112,70,64),
                count~="" and ui.Text{id="packages-label",text="packages",width=0,grow=1,height=24,font_size=kit.tokens.text.body*4/3,color="onSurface"} or nil},
            ui.Text{id="checked",text=checked,width="fill",height=20,font_size=kit.tokens.text.caption*4/3,color=visual.secondary},
            kit.Button{id="update",text="Update now",height=42,padding_left=kit.tokens.spacing.lg,padding_right=kit.tokens.spacing.lg,
                fill="primary",color="onPrimary",radius=21,focus_on_pointer=false,on_activate=open and ui.action("updates.open") or nil}}
    })
end
local function style(title,preferred,render)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render}
end
local function count_style(title,preferred)
    local descriptor=style(title,preferred,bar)
    descriptor.reserve_width=function(_) return item_width end
    return descriptor
end
return {id="updates",title="Updates",icon="deployed_code_update",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon+value",styles={
        icon=style("Icon",1,bar),["icon+value"]=count_style("Icon + value",1),["icon+text"]=style("Icon + text",3,bar),
    }},desktop={default_style="card",styles={
        summary={title="Legacy update summary",min_span=1,max_span=1200,preferred_span=280,preferred_height=80,hidden=true,render=desktop},
        card={title="Update card",min_span=1,max_span=1200,preferred_span=320,preferred_height=160,render=desktop},
        pearl={title="Legacy update panel",min_span=1,max_span=1200,preferred_span=320,preferred_height=160,hidden=true,render=desktop},
        wallpaper={title="Legacy wallpaper count",min_span=1,max_span=1200,preferred_span=280,preferred_height=150,hidden=true,render=desktop}}},
        panel={styles={updates={view="updates"}}}}
}
end
