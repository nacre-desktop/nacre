-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local centered_text=c.centered_text
local header=c.header
local title=c.title
local title_width=c.title_width
local title_min_width=math.min(t.title_max_width,t.title_height*4)
local displays={"window title","app + title","app only","icon"}
local font="IBM Plex Sans"
-- D3 (spec 0237): the title clamps with an ellipsis in the middle so the file name survives.
local function middle_ellipsize(text,width,size)
    if width<=0 then return "" end
    if ui.text_width(text,font,size)<=width+.001 then return text end
    local chars={}
    for _,point in utf8.codes(text) do chars[#chars+1]=utf8.char(point) end
    local function fit(n)
        local head_n=math.ceil(n/2)
        return table.concat(chars,"",1,head_n).."…"..table.concat(chars,"",#chars-(n-head_n)+1,#chars)
    end
    local low,high,best=0,#chars-1,0
    while low<=high do
        local n=math.floor((low+high)/2)
        if ui.text_width(fit(n),font,size)<=width+.001 then best=n; low=n+1 else high=n-1 end
    end
    return fit(best)
end
-- The trace takes the current pearl's colour (context.pearl_accent by workspace index).
local function trace_accent()
    for _,workspace in ui.resolve_for_check(ui.bind("bead.workspaces")) or {} do
        if workspace.active then return c.pearl_accent((workspace.index or 1)-1) end
    end
    return "primary"
end
-- Icon (the real themed app icon from the desktop entry, else the category glyph in secondary) and the
-- accent title (t.title_ink, owner 2026-11-14); `text_width` is the animated label box.
local function title_children(shown,text_width,show_icon,show_text)
    local kids={}
    local visible=ui.bind("bead.desktop.titlevisible")
    if show_icon then
        local icon=ui.resolve_for_check(ui.bind("bead.title.app_icon")) or ""
        -- D3 kit ask 1: the real icon renders untinted until the kit has a palette-tinted monochrome image.
        kids[#kids+1]=icon~="" and ui.Image{id="title-icon",visible=visible,icon=icon,width=t.title_icon,height=t.title_icon,shrink=0}
            or centered_text{id="title-icon",visible=visible,text=ui.bind("bead.title.app_glyph"),width=t.title_icon,shrink=0,family="Material Symbols Rounded",font_size=t.title_icon,color="secondary",text_align="center"}
    end
    if show_text then
        kids[#kids+1]=centered_text{id="title",visible=visible,text=shown,height=24,width=text_width,max_width=text_width,family=font,font_size=t.title_font,color=t.title_ink,text_align="center"}
    end
    return kids
end
-- The 2 px trace under the title, centred in the band 5 px above the bar's bottom edge. Owner 2026-11-14:
-- transient, never a standing underline. Two stable keys chase `bead.title.rev` (the host's title change
-- counter): on each change the trace grows in over 120 ms and fades out over 400 ms (in-cubic, so it holds
-- first). A fresh key or reduced motion snaps to the target, i.e. draws nothing.
local function trace(iid)
    local rev=ui.resolve_for_check(ui.bind("bead.title.rev")) or 0
    local width=t.title_trace_width*(1-math.clamp(rev-ui.animate("title.trace.w."..iid,rev,.12,"out-cubic"),0,1))
    local alpha=.9*math.clamp(rev-ui.animate("title.trace.a."..iid,rev,.4,"in-cubic"),0,1)
    return ui.Row{id="title-trace-row",position="absolute",left=0,right=0,top=header-t.title_trace_inset-t.title_trace_height-(header-t.title_height)/2,height=t.title_trace_height,justify="center",visible=ui.bind("bead.desktop.titlevisible"),
        ui.Box{id="title-trace",width=width,height=t.title_trace_height,radius=1,fill=ui.alpha(trace_accent(),alpha)}}
end
-- Hover shows the full title when it is truncated.
-- Hover shows the full title when it is truncated; the node only exists then, so an ordinary
-- title change still patches one text primitive without a layout.
local function title_tooltip(full,shown)
    if shown==full then return nil end
    local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
    local text_w=math.ceil(ui.text_width(full,font,t.title_font))
    return kit.Tooltip{id="title-tooltip",edge="top",bar_size=-8,width=text_w+2*kit.tokens.spacing.sm,height=30,padding=kit.tokens.spacing.sm,visible=string.find(hovered,"title-band",1,true)~=nil,
        ui.Text{text=full,height=30-2*kit.tokens.spacing.sm,width=text_w,font_size=t.title_font,family=font,color="onSurface"}}
end
local function active_app()
    local title_app=ui.resolve_for_check(ui.bind("bead.title.app_id"))
    if title_app and title_app~="" then return title_app end
    local windows=ui.resolve_for_check(ui.bind("bead.windows")) or {}
    for _,window in windows do
        if window.focused then return window.app_id or "" end
    end
    return ""
end
return {id="title",title="Title",icon="title",category="bar",data={topics={},actions={}},
    settings={
        min_width={type="number",min=0,max=t.title_max_width,step=8},
        max_width={type="number",min=128,max=t.title_max_width,step=8},
        display={type="choice",options=displays},
        behavior={type="choice",options={"hover","click","none"}},
    },
    defaults={min_width=title_min_width,max_width=t.title_max_width,display="window title",behavior="hover"},
    budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="window-title",styles={["window-title"]={title="Window title",min_span=1,max_span=math.ceil((t.title_max_width+t.title_icon+t.title_gap)/32),preferred_span=math.ceil((t.title_max_width+t.title_icon+t.title_gap)/32),reserve_width=function(settings) return (settings.max_width or t.title_max_width)+t.title_icon+t.title_gap end,render=function(props,settings,data)
            settings=settings or {}
            local max_width=settings.max_width or t.title_max_width
            local min_width=math.min(settings.min_width or title_min_width,max_width)
            local display=settings.display or "window title"
            local behavior=settings.behavior or "hover"
            assert(min_width<=max_width,"title minimum width exceeds maximum")
            local full=ui.resolve_for_check(title) or ""
            if min_width==title_min_width and max_width==t.title_max_width and display=="window title" and behavior=="hover"
                and not props.local_layout and not props.available_width and props.instance.host=="top_bar" and props.instance.slot.region=="centre" then
            local animated_width=ui.animate("title.width."..props.instance.iid,title_width,.18,"out-cubic")
            local animated_cue=math.min(t.title_cue_max,animated_width+t.title_icon+t.title_gap+10)
            -- Keep the bound title reactive; resolve its text only when it exceeds
            -- the configured max-width clipping bound and needs an ellipsis.
            local shown=title
            if ui.text_width(title,font,t.title_font)>animated_width then
                shown=middle_ellipsize(full,animated_width,t.title_font)
            end
            return ui.Group{ui.Row{id="title-cue",position="absolute",left="28%",right="28%",top=7,height=36,justify="center",visible=ui.bind("bar.title.cue.visible"),
        ui.Box{id="cue",width=animated_cue+4,height=36,radius=18,fill=ui.alpha("primary",ui.bind("bar.title.cue.halo")),
            ui.Box{id="body",position="absolute",left=2,top=2,width=animated_cue,height=32,radius=16,fill=ui.alpha("primary",ui.bind("bar.title.cue.body")),border_width=1,border_color=ui.alpha("primary",ui.bind("bar.title.cue.border"))},
        },
    },
    ui.Button{id="title-band",position="absolute",left="28%",right="28%",top=(header-t.title_height)/2,height=t.title_height,direction="row",justify="center",align="center",fill="transparent",hover_fill="transparent",focus_ring=false,focus_on_pointer=false,activate_on_release=true,edge_hover=true,aria_label=ui.tr("bead.bar.active-window"),on_activate=ui.action("bar.title"),
        -- The drag handle covers the band; it must stay part of the edge-hover target.
        ui.Row{id="title-content",width=animated_width+t.title_icon+t.title_gap,height=t.title_height,justify="center",align="center",gap=t.title_gap,drag_payload=props.instance.iid,edge_hover=true,
            table.unpack(title_children(shown,animated_width,true,true)),
        },
        trace(props.instance.iid),
        title_tooltip(full,type(shown)=="string" and shown or full),
        kit.Tooltip{id="tray-tooltip",edge="top",bar_size=-8,left=0,width="100%",height=30,padding=3,visible=ui.bind("bead.desktop.tray.tooltip.visible"),
            ui.Text{text=ui.bind("bead.desktop.tray.tooltip"),height=24,width="100%",font_size=13,color="onSurface"},
        },
        ui.Repeat{id="menu",model=ui.bind("bead.desktop.menu"),key="id",render=function(item)
            return kit.Button{id="entry",text=item.label,height=30,padding=4,font_size=13,on_activate=ui.action("services.menu",item.id)}
        end},
    }}
            end
            local show_text=display~="icon"
            local show_icon=true
            local icon_width=show_icon and t.title_icon or 0
            local gap=show_icon and show_text and t.title_gap or 0
            local title_text=show_text and full or ""
            local app=active_app()
            if display=="app only" then title_text=app
            elseif display=="app + title" and app~="" then title_text=app.." · "..title_text end
            local centered=props.instance.host=="top_bar" and props.instance.slot.region=="centre" and not props.local_layout
            local available=centered and ((ui.resolve_for_check(ui.bind("bead.viewport.width")) or 1920)*.44) or max_width
            if props.available_width then available=math.min(available,props.available_width) end
            local capacity=math.max(0,math.min(max_width,available))
            -- Whole pixels, as the label box must be (context.whole_width).
            local text_capacity=math.max(0,math.floor(capacity-icon_width-gap))
            local natural_width=show_text and math.max(0,math.min(text_capacity,
                math.ceil(math.max(min_width-icon_width-gap,ui.text_width(title_text,font,t.title_font))))) or 0
            -- Keep the allocation at its configured maximum reservation while the
            -- rendered child follows the natural title width smoothly.
            local animated_text_width=show_text and ui.animate("title.width."..props.instance.iid,natural_width,.18,"out-cubic") or 0
            -- A shrinking max width takes effect immediately even while the existing
            -- natural-width animation is still moving toward its new target.
            local text_width=math.max(0,math.min(c.whole_width(animated_text_width),text_capacity))
            local shown=show_text and middle_ellipsize(title_text,text_width,t.title_font) or ""
            local content_width=math.min(capacity,math.max(math.min(min_width,capacity),icon_width+gap+text_width))
            local action=behavior=="click" and ui.action("view.open","dashboard-today")
                or behavior=="hover" and ui.action("bar.title") or nil
            local kids={}
            if centered then
                local cue=math.min(t.title_cue_max,content_width+10)
                table.insert(kids,ui.Row{id="title-cue",position="absolute",left="28%",right="28%",top=7,height=36,justify="center",visible=ui.bind("bar.title.cue.visible"),
                    ui.Box{id="cue",width=cue+4,height=36,radius=18,fill=ui.alpha("primary",ui.bind("bar.title.cue.halo")),
                        ui.Box{id="body",position="absolute",left=2,top=2,width=cue,height=32,radius=16,fill=ui.alpha("primary",ui.bind("bar.title.cue.body")),border_width=1,border_color=ui.alpha("primary",ui.bind("bar.title.cue.border"))}}})
            end
            local content={id="title-content",width=content_width,height=t.title_height,gap=gap,align="center",justify="center",
                drag_payload=props.instance.iid,edge_hover=behavior=="hover",clip_children=true}
            for _,kid in title_children(shown,text_width,show_icon,show_text) do table.insert(content,kid) end
            local row={id=centered and "title-band" or "title-local",height=t.title_height,direction="row",gap=0,align="center",justify="center",
                edge_hover=behavior=="hover",activate_on_release=true,focus_on_pointer=false,on_activate=action,aria_label=ui.tr("bead.bar.active-window")}
            if centered then
                row.position="absolute"; row.left="28%"; row.right="28%"; row.top=(header-t.title_height)/2
                row.fill="transparent"; row.hover_fill="transparent"; row.focus_ring=false
            else row.width=content_width end
            table.insert(row,ui.Row(content))
            if centered and show_text then
                table.insert(row,trace(props.instance.iid))
                table.insert(row,title_tooltip(title_text,shown))
            end
            table.insert(row,kit.Tooltip{id="tray-tooltip",edge="top",bar_size=-8,left=0,width="100%",height=30,padding=3,visible=ui.bind("bead.desktop.tray.tooltip.visible"),
                ui.Text{text=ui.bind("bead.desktop.tray.tooltip"),height=24,width="100%",font_size=13,color="onSurface"}})
            table.insert(row,ui.Repeat{id="menu",model=ui.bind("bead.desktop.menu"),key="id",render=function(item)
                return kit.Button{id="entry",text=item.label,height=30,padding=4,font_size=13,on_activate=ui.action("services.menu",item.id)}
            end})
            table.insert(kids,ui.Button(row))
            kids.id="title-root"
            return ui.Group(kids)
    end}}}},panel={styles={dashboard={view="dashboard-today"}}}}
