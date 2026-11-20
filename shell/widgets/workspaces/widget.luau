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
            -- Stack pearls (locked 2026-11-17): one enamel pearl per stack on every screen, in stack order. The
            -- current one is larger, in a white ring, an accent ring and a glow. At rest 1-3 stacks show whole and
            -- 4+ show three around the current one and "+N"; hovering 400 ms fans out numbered pearls (up to 8).
            -- A named 28 px cap (Settings > Panels > Top bar > Stack style) replaces them. Tier 2 advertises no
            -- collections, so neither appears and the pill starts at the current pearl.
            local show_collections=#collections>0
            local cap_style=ui.resolve_for_check(ui.bind("bar.stack.style"))=="name"
            local cur=1
            for i,collection in ipairs(collections) do if collection.id==collection_current then cur=i end end
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
            local limit=fanned and t.stack_fan_max or t.stack_collapsed_max
            local first=#collections>limit and math.clamp(cur-limit//2,1,#collections-limit+1) or 1
            local last=math.min(#collections,first+limit-1)
            local more=#collections-(last-first+1)
            local more_text="+"..more
            local more_width=more>0 and math.ceil(ui.text_width(more_text,stack_family,t.cap_font))+t.stack_gap or 0
            local ring_total=t.stack_ring+t.stack_accent_ring
            -- Size, outer diameter (with the rings) and slot (outer + gap) of stack i's pearl.
            local function stack_size(i)
                local size=fanned and t.stack_fan_size or i==cur and t.stack_current or t.stack_other
                local outer=i==cur and size+2*ring_total or size
                return size,outer,outer+t.stack_gap
            end
            local strand_width=more_width
            for i=first,last do strand_width+=select(3,stack_size(i)) end
            local cap_name=cap_style and measure.ellipsize(current_collection_name,math.ceil(ui.text_width(string.rep("n",t.cap_chars),stack_family,t.cap_font)),stack_family,t.cap_font) or ""
            local cap_name_width=cap_style and math.ceil(ui.text_width(cap_name,stack_family,t.cap_font)) or 0
            local stack_control_width=cap_style and t.cap_pad_left+t.cap_icon+t.cap_gap+cap_name_width+t.cap_pad_right or strand_width+2*t.strand_pad
            -- The control's trailing pad plus the capsule gap puts the current pearl 6 px after the strand.
            local collection_width=show_collections and stack_control_width+2*capsule_gap+1 or 0
            -- The tooltip's centre: the current stack square (the whole control in the name style), in capsule coordinates.
            -- The tooltip is the capsule's height and radius, so it covers what is under it as one layer.
            local tip_width=math.max(t.pill_height,math.ceil(ui.text_width(current_collection_name,stack_family,t.cap_font))+spacing.sm*2)
            local tip_x=5+stack_control_width/2
            if not cap_style then
                tip_x=5+t.strand_pad
                for i=first,cur-1 do tip_x+=select(3,stack_size(i)) end
                tip_x+=select(3,stack_size(cur))/2
            end
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
                    active_left=5+collection_width+cursor+gs
                    active_width=width
                    active_accent=pearl_accent((item.index or 1)-1)
                    active_icon_mode=item.icon_mode==true
                end
                cursor+=gs+width+capsule_gap
            end
            local natural_width=10+cursor-(#workspaces>0 and capsule_gap or 0)+collection_width
            local pill_width=ui.animate("workspaces.pill.width:"..props.instance.iid,natural_width,.16,"out-cubic")
            -- One stack's pearl layers, centred in a slot that starts at x.
            local function stack_nodes(i,k,x)
                local size,outer,slot=stack_size(i)
                local cx,cy,on=x+slot/2,t.cap_height/2,i==cur
                local nodes={}
                -- Stacked cards: a rounded square on a fainter card peeking out behind it; the current one carries the rings and glow.
                local function card(id,w,h,dx,fill,opacity)
                    return ui.Box{id=id,position="absolute",left=cx-w/2+dx,top=cy-h/2,width=w,height=h,radius=w*t.stack_corner,fill=fill,opacity=opacity}
                end
                if on then
                    nodes[1]=card("stack-glow",outer+2*t.stack_glow,outer+2*t.stack_glow,0,ui.alpha("primary",.3))
                    nodes[2]=card("stack-accent",outer,outer,0,"primary")
                    nodes[3]=card("stack-white",size+2*t.stack_ring,size+2*t.stack_ring,0,t.pearl_white)
                end
                nodes[#nodes+1]=card("stack-back-"..k,size,size,2,ui.alpha(pearl_accent(i-1),.5),not on and t.stack_other_alpha or nil)
                nodes[#nodes+1]=card("stack-pearl-"..k,size,size,-1,pearl_accent(i-1),not on and t.stack_other_alpha or nil)
                if fanned then
                    nodes[#nodes+1]=centered_text{id="stack-number-"..k,position="absolute",left=cx-size/2,top=cy-10*t.line_height/2,text=tostring(i),width=size,font_size=10,font_weight=600,family="IBM Plex Sans",text_align="center",color=t.pearl_dark}
                end
                return nodes
            end
            local strand_nodes,fan_buttons={},{}
            if show_collections and not cap_style then
                local x=0
                for i=first,last do
                    local slot=select(3,stack_size(i))
                    local k=i-first+1
                    if not fanned then for _,node in stack_nodes(i,k,x) do strand_nodes[#strand_nodes+1]=node end end
                    if fanned then
                        -- Fanned, each pearl is its own button: the current one keeps the fan out, the others go. Only "+N" (and the cap style) opens the switcher.
                        fan_buttons[k]=kit.Button{id="stack-strand-"..k,width=slot,height=t.cap_height,padding=0,radius=t.cap_radius,shrink=0,fill="transparent",hover_fill="transparent",
                            aria_label=tostring(collections[i].name or collections[i].label or collections[i].index or i),
                            on_activate=c.popouts and (i==cur and ui.action("collection.fan") or ui.action("collection.switch",collections[i].id)) or nil,
                            table.unpack(stack_nodes(i,k,0))}
                    end
                    x+=slot
                end
                if more>0 then
                    local chip=ui.Text{id="stack-more",position="absolute",left=x,top=(t.cap_height-t.cap_font*t.line_height)/2,text=more_text,width=more_width,height=t.cap_font*t.line_height,font_size=t.cap_font,family=stack_family,color="ink2"}
                    strand_nodes[#strand_nodes+1]=chip
                    fan_buttons[#fan_buttons+1]=kit.Button{id="stack-strand-more",width=more_width,height=t.cap_height,padding=0,radius=t.cap_radius,shrink=0,fill="transparent",hover_fill="transparent",
                        aria_label=current_collection_label,on_activate=c.popouts and ui.action("collection.switcher.open") or nil,
                        ui.Text{id="stack-more-text",position="absolute",left=0,top=(t.cap_height-t.cap_font*t.line_height)/2,text=more_text,width=more_width,height=t.cap_font*t.line_height,font_size=t.cap_font,family=stack_family,color="ink2"}}
                end
            end
            local collection_control=show_collections and (fanned and ui.Row{id="stack-fan",width=stack_control_width,height=t.cap_height,padding_left=t.strand_pad,radius=t.cap_radius,fill="surfaceContainerHighest",shrink=0,table.unpack(fan_buttons)} or kit.Button{id="collection-badge",width=stack_control_width,height=t.cap_height,padding=0,radius=t.cap_radius,shrink=0,
                    fill="surfaceContainerHighest",hover_fill="surfaceBright",aria_label=current_collection_label,on_activate=c.popouts and ui.action((cap_style or #collections<2) and "collection.switcher.open" or "collection.fan") or nil,
                    cap_style and ui.Row{id="collection-content",width=stack_control_width,height=t.cap_height,padding_left=t.cap_pad_left,padding_right=t.cap_pad_right,gap=t.cap_gap,align="center",
                        ui.Text{id="collection-icon",text="stacks",width=t.cap_icon,height=t.cap_icon,shrink=0,family=kit.tokens.fonts.icon,font_size=t.cap_icon,color="onSurfaceVariant",text_align="center"},
                        ui.Text{id="collection-name",text=cap_name,width=cap_name_width,height=math.ceil(ui.text_line_height(stack_family,t.cap_font)),shrink=0,font_size=t.cap_font,family=stack_family,color="onSurface"}}
                    or ui.Box{id="strand",position="absolute",left=t.strand_pad,top=0,width=strand_width,height=t.cap_height,table.unpack(strand_nodes)}})
                or nil
            return ui.Row{id="capsule",width=pill_width,height=t.pill_height,padding_left=5,padding_right=5,gap=capsule_gap,radius=t.pill_radius,fill=c.islands and "transparent" or c.base,clip_children=false,align="center",
            -- QML workspaceThread: a 1 px primary -> secondary -> tertiary line at 12 % behind the pearls, inset 16 px.
            thread and kit.tokens.gradients and ui.Image{id="thread",position="absolute",left=21+collection_width,right=21,top=(t.pill_height-1)/2,height=1,path="bar/assets/workspace-thread.themed.svg"} or ui.Box{id="thread",width=0,height=0,shrink=0,visible=false},
            active_left and ui.Box{id="active-capsule",position="absolute",left=ui.animate("workspaces.active.left:"..props.instance.iid,active_left,.18,"out-cubic"),top=(t.pill_height-t.active_height)/2,width=ui.animate("workspaces.active.width:"..props.instance.iid,active_width,.16,"out-cubic"),height=t.active_height,radius=t.active_radius,
                fill=active_icon_mode and active_accent or ui.tint(c.base,active_accent,.15),border_width=1,border_color=ui.alpha(active_accent,.24)} or ui.Box{id="active-capsule",width=0,height=0,visible=false,shrink=0},
            -- The frame variant's own lacquer pill gets its lit edge here (the no-border island draws it).
            c.lit_edge(t.pill_radius,not c.islands),
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
                    animated_width=ui.animate("workspace.cell.width:"..props.instance.iid..":"..item.id,width,.16,"out-cubic")
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
                    animated_width=ui.animate("workspace.cell.width:"..props.instance.iid..":"..item.id,width,.16,"out-cubic")
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
                return ui.Box{id="cell",width=gs+animated_width,height=t.workspace_height,shrink=0,table.unpack(kids)}
            end},
            -- The stack name is one hover away: a tooltip on the strand (or on an ellipsized cap).
            show_collections and kit.Tooltip{id="collection-tooltip",edge="top",bar_size=-spacing.sm,left=math.max(0,tip_x-tip_width/2),width=tip_width,height=t.pill_height,radius=t.pill_radius,padding=spacing.sm,visible=stack_hovered and not fanned and (not cap_style or cap_name~=current_collection_name),
                ui.Text{text=current_collection_name,height=t.pill_height-spacing.sm*2,width="fill",text_align="center",font_size=t.cap_font,family=stack_family,color="onSurface"}} or nil,
        }
    end}}}}}
