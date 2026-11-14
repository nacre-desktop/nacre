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
            local collections={}
            local collection_current=""
            if c.popouts then
                collections=ui.resolve_for_check(ui.bind("bar.collections")) or {}
                collection_current=ui.resolve_for_check(ui.bind("bar.collection.current")) or ""
            end
            -- D2 (spec 0237): the stack folds into the pill as a strand of tiny pearls (default) or a
            -- named 28 px cap (Settings > Panels > Top bar > Stack style). Tier 2 advertises no
            -- collections, so neither appears and the pill starts at the current pearl.
            local show_collections=#collections>0
            local cap_style=ui.resolve_for_check(ui.bind("bar.stack.style"))=="name"
            local current_collection_name=""
            for i,collection in ipairs(collections) do
                if collection.id==collection_current then
                    current_collection_name=collection.name or collection.label or tostring(collection.index or i)
                end
            end
            if current_collection_name=="" and #collections>0 then
                local first=collections[1]
                current_collection_name=first.name or first.label or tostring(first.index or 1)
            end
            local current_collection_label=show_collections and ui.tr("bead.bar.collection-open",{desk=current_collection_name}) or "Collection"
            local spacing=kit.tokens.spacing
            local stack_family=kit.tokens.fonts.sans
            local capsule_gap=spacing.xs/2
            local strand_count=math.min(t.strand_max,#collections)
            local strand_width=t.strand_current+(strand_count-1)*(t.strand_other-t.strand_overlap)
            local cap_name=cap_style and measure.ellipsize(current_collection_name,math.ceil(ui.text_width(string.rep("n",t.cap_chars),stack_family,t.cap_font)),stack_family,t.cap_font) or ""
            local cap_name_width=cap_style and math.ceil(ui.text_width(cap_name,stack_family,t.cap_font)) or 0
            local stack_control_width=cap_style and t.cap_pad_left+t.cap_icon+t.cap_gap+cap_name_width+t.cap_pad_right or strand_width+2*t.strand_pad
            -- The control's trailing pad plus the capsule gap puts the current pearl 6 px after the strand.
            local collection_width=show_collections and stack_control_width+capsule_gap or 0
            local active_left,active_width,active_accent,active_icon_mode
            local cursor=0
            for _,item in workspaces do
                local gs=item.group_start==true and 48 or 0
                local width
                if item.icon_mode then
                    width=46
                else
                    local shown=item.shown or 0
                    local more=item.overflow or ""
                    width=32+shown*18+(more~="" and 20 or 0)+(item.hinted==true and 4 or 0)
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
            local pressed=ui.resolve_for_check(ui.bind("bead.pressed"))==true
            local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
            local stack_hovered=string.find(hovered,"collection-badge",1,true)~=nil
            local pearls={}
            -- The bright pearl is the current stack and leads the strand; the dim ones follow,
            -- overlapping it, and it paints last so it stays whole.
            for i=strand_count,1,-1 do
                local on=i==1
                local size=on and t.strand_current or t.strand_other
                pearls[#pearls+1]=ui.Box{id="stack-pearl-"..i,position="absolute",left=(i-1)*(t.strand_other-t.strand_overlap)+(on and 0 or (t.strand_current-t.strand_other)/2),top=(t.workspace_height-size)/2,width=size,height=size,radius=size/2,
                    fill=on and ui.tint(pearl_accent(0),t.pearl_cream,.4) or "outlineVariant",border_width=t.strand_border,border_color="surfaceContainer"}
            end
            local collection_control=show_collections and kit.Button{id="collection-badge",width=stack_control_width,height=cap_style and t.cap_height or t.workspace_height,padding=0,radius=cap_style and t.cap_radius or t.workspace_radius,shrink=0,
                    fill=cap_style and "surfaceContainerHighest" or "transparent",hover_fill=pressed and "surfaceBright" or cap_style and "surfaceBright" or ui.alpha("onSurface",.08),aria_label=current_collection_label,on_activate=c.popouts and ui.action("collection.switcher.open") or nil,
                    cap_style and ui.Row{id="collection-content",width=stack_control_width,height=t.cap_height,padding_left=t.cap_pad_left,padding_right=t.cap_pad_right,gap=t.cap_gap,align="center",
                        ui.Text{id="collection-icon",text="stacks",width=t.cap_icon,height=t.cap_icon,shrink=0,family=kit.tokens.fonts.icon,font_size=t.cap_icon,color="onSurfaceVariant",text_align="center"},
                        ui.Text{id="collection-name",text=cap_name,width=cap_name_width,height=math.ceil(ui.text_line_height(stack_family,t.cap_font)),shrink=0,font_size=t.cap_font,family=stack_family,color="onSurface"}}
                    or ui.Box{id="strand",position="absolute",left=t.strand_pad,top=0,width=strand_width,height=t.workspace_height,table.unpack(pearls)}}
                or nil
            return ui.Row{id="capsule",width=pill_width,height=t.pill_height,padding_left=5,padding_right=5,gap=capsule_gap,radius=t.pill_radius,fill="surfaceContainer",clip_children=false,align="center",
            -- QML workspaceThread: a 1 px primary -> secondary -> tertiary line at 12 % behind the pearls, inset 16 px.
            thread and kit.tokens.gradients and ui.Image{id="thread",position="absolute",left=21+collection_width,right=21,top=(t.pill_height-1)/2,height=1,path="bar/assets/workspace-thread.themed.svg"} or ui.Box{id="thread",width=0,height=0,shrink=0,visible=false},
            active_left and ui.Box{id="active-capsule",position="absolute",left=ui.animate("workspaces.active.left:"..props.instance.iid,active_left,.18,"out-cubic"),top=(t.pill_height-t.active_height)/2,width=ui.animate("workspaces.active.width:"..props.instance.iid,active_width,.16,"out-cubic"),height=t.active_height,radius=t.active_radius,
                fill=active_icon_mode and active_accent or ui.tint("surfaceContainer",active_accent,.15),border_width=1,border_color=ui.alpha(active_accent,.24)} or ui.Box{id="active-capsule",width=0,height=0,visible=false,shrink=0},
            -- Keep the child list dense when no stack is advertised, so Repeat still renders.
            collection_control or ui.Box{id="collection-segment",width=0,height=0,visible=false,shrink=0},
            ui.Repeat{id="workspaces",model=ui.bind("bead.workspaces"),key="id",render=function(item)
                local accent=pearl_accent((item.index or 1)-1)
                local label=item.label or item.name or ""
                local on=item.active
                local pressed_workspace=pressed and string.find(hovered,".k"..item.id..".",1,true)~=nil
                local activate=ws_on and ui.action("backend.workspace.switch",item.id) or nil
                -- A visiting workspace group starts with its home output's name (QML groupSpace, 48 px).
                local gs=item.group_start==true and 48 or 0
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
                    local shown=item.shown or 0
                    local more=item.overflow or ""
                    local hinted=item.hinted==true
                    local apps={}
                    -- Owner 2026-11-14: Settings > Top bar > Workspace style "App icons" shows the real app icons
                    -- (the title's lookup, category glyph when an app has none); "Show all" lifts the three-hint cap.
                    for slot=1,math.max(3,shown) do
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
                    -- QML NacreWorkspacePearl: 22 px current / 20 occupied / 18 empty. The enamel is a flat accent warmed toward
                    -- cream with a neutral cream/secondary shading and specular highlight over it, so any blended accent works.
                    -- Gradients on: the plain accent under the shading; off: the flat warmed colour (QML numberSurface).
                    local shaded=lit and kit.tokens.gradients
                    add(ui.Box{id="pearl",position="absolute",left=16-size/2,top=t.workspace_height/2-size/2,width=size,height=size,radius=size/2,fill=lit and (shaded and accent or ui.tint(accent,t.pearl_cream,on and .4 or .1)) or "surfaceContainer"})
                    if shaded then
                        add(ui.Image{id="enamel-gradient",position="absolute",left=16-size/2,top=t.workspace_height/2-size/2,width=size,height=size,
                            path="bar/assets/pearl-"..(on and "current" or "occupied")..".themed.svg"})
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
                    kids[#kids+1]=ui.Text{id="group-origin",position="absolute",left=10,top=5,width=36,height=18,text="▣ "..(item.origin or ""),font_size=11,color="onSurfaceVariant"}
                end
                kids[#kids+1]=button
                for _,overlay in ipairs(overlays) do kids[#kids+1]=overlay end
                if pressed_workspace then
                    kids[#kids+1]=ui.Box{id="pressed-feedback",position="absolute",left=gs,top=0,width=animated_width,height=t.workspace_height,radius=t.workspace_radius,fill=ui.alpha("onSurface",.12),shrink=0}
                end
                return ui.Box{id="cell",width=gs+animated_width,height=t.workspace_height,shrink=0,table.unpack(kids)}
            end},
            -- The stack name is one hover away: a tooltip on the strand (or on an ellipsized cap).
            show_collections and kit.Tooltip{id="collection-tooltip",edge="top",bar_size=-spacing.sm,left=0,width=math.ceil(ui.text_width(current_collection_name,stack_family,t.cap_font))+spacing.sm*2,height=t.workspace_height,padding=spacing.sm,visible=stack_hovered and (not cap_style or cap_name~=current_collection_name),
                ui.Text{text=current_collection_name,height=t.workspace_height-spacing.sm*2,width="fill",font_size=t.cap_font,family=stack_family,color="onSurface"}} or nil,
        }
    end}}}}}
