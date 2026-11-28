-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local measure=require("bead.widgets.measure")
local t=c.t
local centered_text=c.centered_text
local ws_on=c.ws_on
local focus_on=c.focus_on
local thread=c.thread
local pearl_accent=c.pearl_accent
-- Out-cubic `ui.animated` slide of `target`, plus `base`; a plain number while it does not move, so a resting bar holds no markers.
local function slide(key,target,duration,base)
    local drive=ui.animated(key,target,duration,"out-cubic")
    local from,to=ui.range(drive)
    if from==to then return base+to end
    -- Luau cannot add to a marker: follow the driver on a track.
    return ui.track(drive,{math.min(from,to),math.max(from,to)},from<to and {base+from,base+to} or {base+to,base+from})
end
return {id="workspaces",title="Workspaces",icon="view_carousel",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="pearls",styles={pearls={title="Workspaces",min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            local workspaces=ui.resolve_for_check(ui.bind("bead.workspaces")) or {}
            -- Enamel pearl: the accent fill under the themed gloss (tinted from the wallpaper's palette); no gradients, the flat fill.
            local function enamel(id,left,top,size,fill,current,opacity)
                local out={ui.Box{id=id,position="absolute",left=left,top=top,width=size,height=size,radius=size/2,fill=fill,opacity=opacity}}
                if kit.tokens.gradients then
                    out[2]=ui.Image{id=id.."-gradient",position="absolute",left=left,top=top,width=size,height=size,opacity=opacity,path="bar/assets/pearl-"..(current and "current" or "occupied")..".themed.svg"}
                end
                return out
            end
            -- App icons ride only in the active pearl, at every output width.
            local function hints(item)
                return item.shown or 0,item.overflow or "",item.hinted==true
            end
            local collections={}
            local collection_current=""
            if c.popouts then
                collections=ui.resolve_for_check(ui.bind("bar.collections")) or {}
                collection_current=ui.resolve_for_check(ui.bind("bar.collection.current")) or ""
                -- Stacks belong to an output (nacre-niri), and a switch swaps them between screens, so every bar lists all of them
                -- in stack order: the highlight then visibly moves when this output's stack changes.
            end
            -- Stack circles (spec 0253, owner pick A 2026-11-25): one circle per stack in a 28 px cap, left to right and
            -- overlapping like a Venn row, this output's current stack lit. With 2+ stacks hovering 400 ms (or a click) fans out
            -- every stack's numeral as its own button (up to 8, then "+N"). A named 28 px cap (Settings > Panels > Top bar >
            -- Stack style) replaces it. Tier 2 advertises no collections, so neither appears and the pill starts at the current pearl.
            local show_collections=#collections>0
            local cap_style=ui.resolve_for_check(ui.bind("bar.stack.style"))=="name"
            local cur=1
            for i,collection in ipairs(collections) do if collection.id==collection_current then cur=i end end
            -- The number the switcher shows ("Stack N"), else the stack's place in the list.
            local function stack_number(i) return tostring(collections[i] and collections[i].index or i) end
            local current_collection_name=""
            if collections[cur] then
                local current=collections[cur]
                current_collection_name=current.name or current.label or tostring(current.index or cur)
            end
            local current_collection_label=show_collections and ui.tr("bead.bar.collection-open",{desk=current_collection_name}) or "Collection"
            local spacing=kit.tokens.spacing
            local stack_family=kit.tokens.fonts.sans
            local capsule_gap=spacing.xs/2
            local pressed=ui.resolve_for_check(ui.bind("bead.pressed"))==true
            local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
            local stack_hovered=string.find(hovered,"collection-badge",1,true)~=nil or string.find(hovered,"stack-strand",1,true)~=nil
            -- The open waits for the 400 ms hover; the close is at once.
            local fan=ui.animate("workspaces.stack.fan:"..props.instance.iid,stack_hovered and 1 or 0,.4,"linear")
            -- A click on the strand (bar.stack.fan counts them) fans at once and keeps it out for 3 s after the pointer leaves.
            local clicks=c.popouts and ui.resolve_for_check(ui.bind("bar.stack.fan")) or 0
            -- The counter ramps in 3 s on a stable key (a fresh key snaps), so the fan is pinned until it catches up.
            local pinned=ui.animate("workspaces.stack.pin:"..props.instance.iid,clicks,3,"linear")<clicks-.001
            local fanned=not cap_style and #collections>1 and (fan>=.999 or pinned)
            local limit=fanned and t.stack_fan_max or t.stack_circles_max
            local first=#collections>limit and math.clamp(cur-limit//2,1,#collections-limit+1) or 1
            local last=math.min(#collections,first+limit-1)
            local more=#collections-(last-first+1)
            local more_text="+"..more
            local more_width=more>0 and math.ceil(ui.text_width(more_text,stack_family,t.cap_font))+t.stack_gap or 0
            local cap_name=cap_style and measure.ellipsize(current_collection_name,math.ceil(ui.text_width(string.rep("n",t.cap_chars),stack_family,t.cap_font)),stack_family,t.cap_font) or ""
            local cap_name_width=cap_style and math.ceil(ui.text_width(cap_name,stack_family,t.cap_font)) or 0
            local stack_control_width=cap_style and t.cap_pad_left+t.cap_icon+t.cap_gap+cap_name_width+t.cap_pad_right or fanned and (last-first+1)*t.stack_slot+more_width+2*t.strand_pad or t.cap_height+(last-first)*(t.stack_circle-t.strand_overlap)
            -- The control's trailing pad plus the capsule gap puts the current pearl 6 px after the strand.
            local collection_width=show_collections and stack_control_width+2*capsule_gap+1 or 0
            local active_left,active_width,active_accent,active_icon_mode
            local cursor=0
            for _,item in workspaces do
                local gs=item.group_start==true and 12 or 0
                local width
                if item.icon_mode then
                    width=46
                else
                    local shown,overflow,hinted=hints(item)
                    width=32+shown*18+(overflow~="" and 20 or 0)+(hinted and 4 or 0)
                end
                if item.active then
                    active_left=cursor+gs
                    active_width=width
                    active_accent=pearl_accent((item.index or 1)-1)
                    active_icon_mode=item.icon_mode==true
                end
                cursor+=gs+width+capsule_gap
            end
            local natural_width=10+cursor-(#workspaces>0 and capsule_gap or 0)
            -- The stack control's width (the fan opening or closing) moves the workspaces at once, so the pill and the
            -- active capsule take it at once too; only the workspaces' own changes slide (a trailing capsule was a dark circle).
            local pill_width=slide("workspaces.pill.width:"..props.instance.iid,natural_width,.16,collection_width)
            -- At rest the circles (at most `stack_circles_max`, around the current one): dim discs, then the lights on top. A
            -- light's alpha per slot is a `ui.animated` marker, so a switch eases it from the old circle to the new one with no Luau
            -- per frame; a plain number at rest, where only the current slot draws one. Built only while stacks show, so the
            -- first frame with stacks (startup, a drag ghost) snaps.
            local circles={}
            if show_collections and not fanned and not cap_style then
                local lights={}
                for i=first,last do
                    local k=i-first+1
                    local function circle(id,fill)
                        return ui.Box{id=id,position="absolute",left=(t.cap_height-t.stack_circle)/2+(k-1)*(t.stack_circle-t.strand_overlap),top=(t.cap_height-t.stack_circle)/2,width=t.stack_circle,height=t.stack_circle,radius=t.stack_circle/2,fill=fill}
                    end
                    circles[k]=circle("stack-circle-"..k,ui.alpha("onSurfaceVariant",t.stack_dim))
                    local drive=ui.animated("workspaces.stack.light:"..props.instance.iid..":"..k,i==cur and 1 or 0,ui.motion.duration.switch,ui.motion.curve.out_cubic)
                    local from,to=ui.range(drive)
                    if from~=to or to>0 then lights[#lights+1]=circle("stack-lit-"..k,ui.alpha("primary",from==to and to or drive)) end
                end
                for _,light in lights do circles[#circles+1]=light end
            end
            local fan_buttons={}
            if show_collections and fanned then
                for i=first,last do
                    local k=i-first+1
                    -- Fanned, each numeral is its own button: the current one keeps the fan out, the others go. Only "+N" (and the cap style) opens the switcher.
                    fan_buttons[k]=kit.Button{id="stack-strand-"..k,width=t.stack_slot,height=t.cap_height,padding=0,radius=t.cap_radius,shrink=0,fill="transparent",hover_fill="transparent",
                        aria_label=tostring(collections[i].name or collections[i].label or collections[i].index or i),
                        on_activate=c.popouts and (i==cur and ui.action("collection.fan") or ui.action("collection.switch",collections[i].id)) or nil,
                        centered_text{id="stack-number-"..k,position="absolute",left=0,top=(t.cap_height-t.stack_font*t.line_height)/2,width=t.stack_slot,text=stack_number(i),font_size=t.stack_font,font_weight=i==cur and 600 or 500,family=stack_family,text_align="center",color=i==cur and "primary" or "onSurfaceVariant"}}
                end
                if more>0 then
                    fan_buttons[#fan_buttons+1]=kit.Button{id="stack-strand-more",width=more_width,height=t.cap_height,padding=0,radius=t.cap_radius,shrink=0,fill="transparent",hover_fill="transparent",
                        aria_label=current_collection_label,on_activate=c.popouts and ui.action("collection.switcher.open") or nil,
                        ui.Text{id="stack-more-text",position="absolute",left=0,top=(t.cap_height-t.cap_font*t.line_height)/2,text=more_text,width=more_width,height=t.cap_font*t.line_height,font_size=t.cap_font,family=stack_family,color="ink2"}}
                end
            end
            local collection_control=show_collections and (fanned and ui.Row{id="stack-fan",width=stack_control_width,height=t.cap_height,padding_left=t.strand_pad,radius=t.cap_radius,fill="surfaceContainerHighest",shrink=0,table.unpack(fan_buttons)} or kit.Button{id="collection-badge",width=stack_control_width,height=t.cap_height,padding=0,radius=t.cap_radius,shrink=0,
                    fill=cap_style and "surfaceContainerHighest" or "transparent",hover_fill=cap_style and ui.tint("surfaceContainerHighest","onSurface",.03) or ui.alpha("onSurface",.03),aria_label=cap_style and current_collection_label or ui.tr("bead.bar.stack-position",{index=stack_number(cur),count=tostring(#collections)}),on_activate=c.popouts and ui.action((cap_style or #collections<2) and "collection.switcher.open" or "collection.fan") or nil,
                    cap_style and ui.Row{id="collection-content",width=stack_control_width,height=t.cap_height,padding_left=t.cap_pad_left,padding_right=t.cap_pad_right,gap=t.cap_gap,align="center",
                        ui.Text{id="collection-icon",text="stacks",width=t.cap_icon,height=t.cap_icon,shrink=0,family=kit.tokens.fonts.icon,font_size=t.cap_icon,color="onSurfaceVariant",text_align="center"},
                        ui.Text{id="collection-name",text=cap_name,width=cap_name_width,height=math.ceil(ui.text_line_height(stack_family,t.cap_font)),shrink=0,font_size=t.cap_font,family=stack_family,color="onSurface"}}
                    or ui.Box{id="stack-circles",position="absolute",left=0,top=0,width=stack_control_width,height=t.cap_height,table.unpack(circles)}})
                or nil
            return ui.Row{id="capsule",width=pill_width,height=t.pill_height,padding_left=5,padding_right=5,gap=capsule_gap,radius=t.pill_radius,fill=c.islands and "transparent" or c.base,clip_children=false,align="center",
            -- QML workspaceThread: a 1 px primary -> secondary -> tertiary line at 12 % behind the pearls, inset 16 px.
            thread and kit.tokens.gradients and ui.Image{id="thread",position="absolute",left=21+collection_width,right=21,top=(t.pill_height-1)/2,height=1,path="bar/assets/workspace-thread.themed.svg"} or ui.Box{id="thread",width=0,height=0,shrink=0,visible=false},
            active_left and ui.Box{id="active-capsule",position="absolute",left=5+collection_width,translate_x=slide("workspaces.active.left:"..props.instance.iid,active_left,.18,0),top=(t.pill_height-t.active_height)/2,width=slide("workspaces.active.width:"..props.instance.iid,active_width,.16,0),height=t.active_height,radius=t.active_radius,
                fill=active_icon_mode and active_accent or ui.tint(c.base,active_accent,.15),border_width=1,border_color=ui.alpha(active_accent,.24)} or ui.Box{id="active-capsule",width=0,height=0,visible=false,shrink=0},
            -- Keep the child list dense when no stack is advertised, so Repeat still renders.
            collection_control or ui.Box{id="collection-segment",width=0,height=0,visible=false,shrink=0},
            -- A hairline closes the stack capsule off from the workspaces.
            show_collections and ui.Box{id="stack-divider",width=1,height=14,shrink=0,fill="outlineVariant"} or ui.Box{id="stack-divider",width=0,height=0,visible=false,shrink=0},
            ui.Repeat{id="workspaces",model=ui.bind("bead.workspaces"),key="id",render=function(item)
                local accent=pearl_accent((item.index or 1)-1)
                local label=item.label or item.name or ""
                local on=item.active
                local pressed_workspace=pressed and string.find(hovered,".k"..item.id..".",1,true)~=nil
                local activate=ws_on and ui.action("backend.workspace.switch",item.id) or nil
                -- A visiting workspace group starts with a thin rule (no raw connector name, owner 2026-11-18).
                local gs=item.group_start==true and 12 or 0
                local button
                local width
                local animated_width
                local overlays={}
                if item.icon_mode then
                    -- workspaceIcons: 46 px cells, accent fill when active, symbol and number, dot when occupied.
                    local ink=on and "onPrimary" or "onSurfaceVariant"
                    local icon=item.icon or ""
                    width=46
                    animated_width=slide("workspace.cell.width:"..props.instance.iid..":"..item.id,width,.16,0)
                    button=kit.Button{id="workspace",position="absolute",left=gs,top=0,width=animated_width,height=t.workspace_height,radius=t.workspace_radius,padding=0,fill="transparent",hover_fill=ui.alpha(accent,0.1),shrink=0,
                        on_activate=activate,
                        ui.Row{id="face",position="absolute",left=0,top=2,width=46,height=24,gap=4,align="center",justify="center",
                            ui.Text{id="symbol",visible=icon~="",text=icon,width=icon~="" and 18 or 0,height=24,shrink=0,family="Material Symbols Rounded",font_size=15,text_align="center",color=ink},
                            ui.Text{id="number",text=label,width=8*#label,height=24,shrink=0,font_size=13,family="IBM Plex Sans",text_align="center",color=ink},
                        },
                        ui.Box{id="occupied",position="absolute",left=21,top=23,width=3,height=3,radius=2,visible=item.occupied==true,fill=on and "onPrimary" or accent},
                    }
                else
                    -- workspaceWindowHints: category glyphs of the active workspace's windows beside its pearl.
                    local shown,more,hinted=hints(item)
                    local apps={}
                    -- Owner 2026-11-14: Settings > Top bar > Workspace style "App icons" shows the real app icons
                    -- (the title's lookup, category glyph when an app has none); "Show all" lifts the three-hint cap.
                    for slot=1,(shown>0 or hinted) and math.max(3,shown) or 0 do
                        local glyph_name=item["app"..slot]
                        local icon_name=item["icon"..slot] or ""
                        if icon_name~="" then
                            apps[#apps+1]=ui.Box{id="app"..slot,width=15,height=28,shrink=0,
                                ui.Image{id="icon",position="absolute",left=(15-t.hint_icon)/2,top=(28-t.hint_icon)/2,icon=icon_name,width=t.hint_icon,height=t.hint_icon}}
                        elseif glyph_name and glyph_name~="" then
                            apps[#apps+1]=centered_text{id="app"..slot,text=glyph_name,width=15,height=28,shrink=0,family="Material Symbols Rounded",font_size=t.app_font,text_align="center",color="onSurface"}
                            -- Clicking a glyph focuses its window (window.focus); a sibling, as a button cannot nest in a button.
                            local window=item["win"..slot]
                            if focus_on and window and window~="" then
                                overlays[#overlays+1]=kit.Button{id="focus"..slot,position="absolute",left=gs+33+(slot-1)*18,top=0,width=15,height=28,radius=7,padding=0,fill="transparent",hover_fill=ui.alpha("onSurface",0.14),shrink=0,
                                    aria_label=ui.tr("bead.bar.focus-window"),on_activate=ui.action("backend.window.focus",window),
                                    ui.Box{id="hit",width=15,height=28}}
                            end
                        end
                    end
                    if more~="" then
                        apps[#apps+1]=centered_text{id="more",text=more,width=17,shrink=0,font_size=12,color="onSurface"}
                    end
                    width=32+shown*18+(more~="" and 20 or 0)+(hinted and 4 or 0)
                    animated_width=slide("workspace.cell.width:"..props.instance.iid..":"..item.id,width,.16,0)
                    local lit=on or item.occupied==true
                    local size=on and t.pearl_current or lit and t.pearl_occupied or t.pearl_empty
                    local parts={}
                    local function add(node) parts[#parts+1]=node end
                    -- QML NacreWorkspacePearl: 22 px current / 20 occupied / 18 empty; the enamel is the accent warmed toward cream
                    -- under the themed shading (gradients off: the flat warmed colour; QML numberSurface).
                    local shaded=lit and kit.tokens.gradients
                    for _,node in enamel("pearl",16-size/2,t.workspace_height/2-size/2,size,lit and (shaded and accent or ui.tint(accent,t.pearl_cream,on and .4 or .1)) or c.base,on) do
                        if lit or node.id=="pearl" then add(node) end
                    end
                    local stroke=on and t.pearl_current_border or not lit and t.pearl_empty_border or 0
                    if stroke>0 then
                        -- Bead strokes straddle their path; Qt's Rectangle border stays inside its box.
                        add(ui.Box{id="ring",position="absolute",left=16-size/2+stroke/2,top=t.workspace_height/2-size/2+stroke/2,width=size-stroke,height=size-stroke,radius=(size-stroke)/2,
                            fill="transparent",border_width=stroke,border_color=on and ui.alpha("onSurface",.5) or ui.alpha(accent,.9)})
                    end
                    add(centered_text{id="number",position="absolute",left=16-size/2,top=t.workspace_height/2-(#label<2 and t.workspace_number_font or 9)*t.line_height/2,text=label,width=size,font_size=#label<2 and t.workspace_number_font or 9,font_weight=on and 600 or 500,family="IBM Plex Sans",text_align="center",color=lit and t.pearl_dark or "onSurfaceVariant"})
                    add(ui.Row{id="apps",position="absolute",left=33,top=0,width=math.max(0,width-33),height=t.workspace_height,gap=3,align="center",visible=#apps>0,table.unpack(apps)})
                    button=kit.Button{id="workspace",position="absolute",left=gs,top=0,width=animated_width,height=t.workspace_height,radius=t.workspace_radius,padding=0,fill="transparent",hover_fill=ui.alpha(accent,.1),shrink=0,
                        on_activate=activate,
                        table.unpack(parts),
                    }
                end
                local kids={}
                if gs>0 then
                    kids[#kids+1]=ui.Box{id="group-rule",position="absolute",left=5,top=5,width=1,height=18,fill="outlineVariant"}
                end
                kids[#kids+1]=button
                for _,overlay in ipairs(overlays) do kids[#kids+1]=overlay end
                if pressed_workspace then
                    kids[#kids+1]=ui.Box{id="pressed-feedback",position="absolute",left=gs,top=0,width=animated_width,height=t.workspace_height,radius=t.workspace_radius,fill=ui.alpha("onSurface",.12),shrink=0}
                end
                return ui.Box{id="cell",width=(slide("workspace.cell.width:"..props.instance.iid..":"..item.id,width,.16,gs)),height=t.workspace_height,shrink=0,table.unpack(kids)}
            end},
        }
    end}}}}}
