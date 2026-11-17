-- SPDX-License-Identifier: GPL-3.0-only
-- Strand, the wallpaper picker: pearls on a silk thread that hangs from the bar. Moving along the strand
-- changes the real wallpaper in tandem (extras.rs activates every step at once); the shell colours follow it.
-- Nacre routes and settings writes stay in extras.rs.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=require("../tokens")
local W=T.wallpaper
local width=ui.resolve_for_check(ui.bind("extras.viewport.width")) or W.viewport_fallback_width
local height=ui.resolve_for_check(ui.bind("extras.viewport.height")) or W.viewport_fallback_height
local kind=ui.resolve_for_check(ui.bind("extras.wallpaper.kind")) or "static"
local search_focused=ui.resolve_for_check(ui.bind("bead.focused"))=="extras.search"
local rows=ui.resolve_for_check(ui.bind("extras.filtered")) or {}
local count=#rows
local selected
for _,item in ipairs(rows) do if item.selected then selected=item; break end end
local selection_index=selected and selected.selection_index or 1
local can_pause=ui.resolve_for_check(ui.bind("extras.wallpaper.can_pause"))==true
local paused=ui.resolve_for_check(ui.bind("extras.wallpaper.paused"))==true
local resolution=ui.resolve_for_check(ui.bind("extras.wallpaper.resolution")) or ""
local resolution_id=ui.resolve_for_check(ui.bind("extras.wallpaper.resolution_id")) or ""
local position=ui.resolve_for_check(ui.bind("extras.wallpaper.position")) or 1
if math.floor(position-1)%math.max(1,count)+1~=selection_index then position=selection_index end
local animation_query=tostring(ui.resolve_for_check(ui.bind("extras.query")) or "")
local animation_key=2166136261
for index=1,#animation_query do animation_key=(animation_key*33+string.byte(animation_query,index))%2147483647 end
-- One continuous value drives the whole strand: pearls slide along the thread to a fractional slot.
local strand=ui.animate("wallpaper-strand:"..kind..":"..tostring(count)..":"..tostring(animation_key),position,W.move_duration,"out-quint")
-- Opening: the thread and its pearls drop in from the top edge. Swap this one line for another entrance.
local entrance=ui.animate("wallpaper-strand-open",ui.resolve_for_check(ui.bind("panel.open"))==false and 0 or 1,W.open_duration,"out-cubic")

local s=math.min(width/W.art_width,height/W.art_height)
local centre={x=width/2,y=height/2}
local gap=W.pearl_gap*s

-- True catenary through the two off-screen anchors with its lowest point at the centre: a*(cosh(half/a)-1)=sag.
local half=centre.x-W.anchor_x*s
local sag=centre.y-W.anchor_y*s
local lo,hi=50*s,5000*s
for _=1,60 do
  local mid=(lo+hi)/2
  if mid*(math.cosh(half/mid)-1)>sag then lo=mid else hi=mid end
end
local a=(lo+hi)/2
local base=centre.y+a
local function point_at(arc) return centre.x+a*math.log(arc/a+math.sqrt(arc*arc/(a*a)+1)),base-math.sqrt(a*a+arc*arc) end

local sizes=W.pearl_sizes
local function size_at(d)
  local i=math.min(#sizes-2,math.floor(d))
  return (sizes[i+1]+(sizes[i+2]-sizes[i+1])*(d-i))*s
end
-- Arc length from the lowest point to the centre of the pearl `d` slots away (fractional d interpolates).
local function reach(d)
  local length,k=0,0
  while d-k>1 do length=length+(size_at(k)+size_at(k+1))/2+gap; k=k+1 end
  return length+(d-k)*((size_at(k)+size_at(k+1))/2+gap)
end

local DISC={}
for index=0,31 do local angle=index*math.pi/16; DISC[#DISC+1]={.5+.5*math.cos(angle),.5+.5*math.sin(angle)} end

-- The pearls within reach of the animated position, farthest first so the chosen one draws on top.
local placed={}
for index=math.floor(strand-W.visible_reach),math.floor(strand+W.visible_reach)+1 do
  if count>0 then
    local wrapped=((index-1)%count)+1
    local offset=((wrapped-strand+count/2)%count)-count/2
    local seen=false
    for _,known in ipairs(placed) do if known.wrapped==wrapped then seen=true end end
    if not seen and math.abs(offset)<=W.visible_reach then
      local arc=math.sign(offset)*reach(math.abs(offset))
      local x,y=point_at(arc)
      placed[#placed+1]={item=rows[wrapped],wrapped=wrapped,offset=offset,arc=arc,x=x,y=y,size=size_at(math.abs(offset))}
    end
  end
end
table.sort(placed,function(left,right) return math.abs(left.offset)>math.abs(right.offset) end)
local nearest=W.visible_reach
for _,pearl in ipairs(placed) do nearest=math.min(nearest,math.abs(pearl.offset)) end

local function pearl_node(pearl)
  local item,size=pearl.item,pearl.size
  local near=math.max(0,1-math.abs(pearl.offset))
  local edge=math.max(1.5,size*.028)
  local path=item.thumbnail
  local kids={}
  if math.abs(pearl.offset)<=2.5 then
    kids[#kids+1]=ui.Image{id="shadow",position="absolute",left=-size*.12,top=-size*.04,width=size*1.24,height=size*1.24,
      path=W.glow,tint={0,0,0,math.floor(110*math.min(1,size/(W.pearl_sizes[1]*s)+.35))}}
  end
  if path and path~="" then
    kids[#kids+1]=ui.Image{id="thumbnail",position="absolute",left=0,top=0,width=size,height=size,
      icon=path,image_fit="cover",clip_polygon=DISC,aria_label=item.title or "Wallpaper"}
  else
    kids[#kids+1]=ui.Box{id="missing-thumbnail",position="absolute",left=0,top=0,width=size,height=size,radius=size/2,fill="surfaceContainerHigh"}
    kids[#kids+1]=ui.Text{id="missing-icon",position="absolute",left=0,top=size/2-W.thumbnail_fallback_icon*s/2,width=size,height=W.thumbnail_fallback_icon*s,
      text=item.kind=="video" and "movie" or "image",family="Material Symbols Rounded",font_size=W.thumbnail_fallback_icon*s,
      color="onSurfaceVariant",text_align="center"}
  end
  kids[#kids+1]=ui.Box{id="edge",position="absolute",left=0,top=0,width=size,height=size,radius=size/2,fill="transparent",
    border_width=edge,border_color=W.pearl_edge}
  if near>.3 then
    kids[#kids+1]=ui.Image{id="sheen",position="absolute",left=0,top=0,width=size,height=size,path=W.sheen,
      tint={W.pearl_sheen[1],W.pearl_sheen[2],W.pearl_sheen[3],math.floor(W.pearl_sheen[4]*near)}}
  end
  -- The chosen pearl keeps the pick; any other pearl only moves the strand.
  local request=(math.abs(pearl.offset)<.5 and "wallpaper:commit:" or "wallpaper:select:")..tostring(item.id)
  return ui.Button{id="pearl-"..tostring(item.selection_index),position="absolute",left=pearl.x-size/2,top=pearl.y-size/2,
    width=size,height=size,padding=0,radius=size/2,fill="transparent",hover_fill="transparent",
    activate_on_release=true,focus_on_pointer=false,on_activate=ui.action("extras.do",request),
    aria_label=(item.title or "Wallpaper")..(item.active and ", current" or ""),
    ui.Box{width=size,height=size,fill="transparent",table.unpack(kids)}}
end

-- Silk: a dark shadow stroke under five iridescent segments that fade at the ends. Each stroke gets a box
-- that hugs its own piece of the curve, so the CPU renderer never needs a panel-sized repair scratch.
local function silk(id,from,to,colour,stroke)
  local x0,x1,top,bottom=width*from,width*to,math.huge,-math.huge
  local curve={}
  for step=0,12 do
    local x=x0+(x1-x0)*step/12
    local y=base-a*math.cosh((x-centre.x)/a)
    curve[#curve+1]={x,y}
    top,bottom=math.min(top,y),math.max(bottom,y)
  end
  local pad=stroke*s+2
  local left,box_width,box_height=x0-pad,x1-x0+2*pad,bottom-top+2*pad
  local points={}
  for _,point in ipairs(curve) do points[#points+1]={(point[1]-left)/box_width,(point[2]-top+pad)/box_height} end
  return ui.Polyline{id=id,position="absolute",left=left,top=top-pad,width=box_width,height=box_height,points=points,stroke_width=stroke*s,color=colour}
end
local strand_nodes={}
for index in ipairs(W.silk) do strand_nodes[#strand_nodes+1]=silk("thread-shadow-"..index,(index-1)/#W.silk,index/#W.silk,W.silk_shadow,W.silk_stroke+2) end
for index,colour in ipairs(W.silk) do strand_nodes[#strand_nodes+1]=silk("thread-"..index,(index-1)/#W.silk,index/#W.silk,colour,W.silk_stroke) end

local knot=math.max(3,W.knot*s)
table.sort(placed,function(left,right) return left.offset<right.offset end)
for index=1,#placed-1 do
  local left,right=placed[index],placed[index+1]
  if right.offset-left.offset<1.5 then
    local x,y=point_at(((left.arc+left.size/2)+(right.arc-right.size/2))/2)
    strand_nodes[#strand_nodes+1]=ui.Box{id="knot-"..index,position="absolute",left=x-knot/2,top=y-knot/2,width=knot,height=knot,radius=knot/2,fill=W.knot_fill}
  end
end
table.sort(placed,function(left,right) return math.abs(left.offset)>math.abs(right.offset) end)
for _,pearl in ipairs(placed) do strand_nodes[#strand_nodes+1]=pearl_node(pearl) end

-- A thin conic ring (five arcs from pink round to cream) around the lowest point.
local ring=size_at(nearest)+2*(W.ring_gap+W.ring_stroke)*s
local stroke=math.max(1,W.ring_stroke*s)
-- Between two pearls the ring has nothing to hold: it fades, then comes back around the next one.
local ring_alpha=math.floor(255*math.max(0,math.min(1,1-nearest*1.6)))
if count>0 then
  for index,colour in ipairs(W.silk) do
    strand_nodes[#strand_nodes+1]=ui.Arc{id="ring-"..index,position="absolute",left=centre.x-ring/2+stroke/2,top=centre.y-ring/2+stroke/2,
      width=ring-stroke,height=ring-stroke,start_angle=-160+(index-1)*72,sweep_angle=72,stroke_width=stroke,round_caps=false,
      color={colour[1],colour[2],colour[3],ring_alpha}}
  end
end

local function glyph_button(id,label,glyph,size,font,fill,action,request,left,top)
  local props={id=id,aria_label=label,width=size,height=size,shrink=0,padding=0,radius=size/2,fill=fill,hover_fill=fill,
    activate_on_release=true,focus_on_pointer=false,on_activate=ui.action(action,request),
    border_width=fill==W.nav_fill and 1 or nil,border_color=W.nav_ring,
    ui.Row{width=size,height=size,align="center",justify="center",
      ui.Text{text=glyph,width=font,height=font*1.15,family="Material Symbols Rounded",font_size=font,color=W.ink}}}
  if left then props.position,props.left,props.top="absolute",left,top end
  return kit.Button(props)
end
local function toggle(id,label,active,request,toggle_width)
  local size=W.button_size*s
  return kit.Button{id=id,aria_label=label,width=toggle_width*s,shrink=0,height=size,padding=0,radius=size/2,
    fill=active and W.toggle_active or "transparent",hover_fill=active and W.toggle_active or {255,255,255,26},
    activate_on_release=true,focus_on_pointer=false,on_activate=ui.action("extras.do",request),
    ui.Text{position="absolute",left=0,top=(size-T.palette.description_font*s*1.4)/2,width="fill",height=T.palette.description_font*s*1.4,
      text=label,font_size=T.palette.description_font*s,text_align="center",color=active and W.toggle_active_ink or W.ink}}
end

-- Veil: flat translucent bands that follow the design gradient (.55 top, .30 at 40 %, .62 bottom). A stretched
-- SVG would be rasterised at the full panel size, which overruns Bead's retained raster budget at 2880x1800.
local function veil_alpha(at) return at<.4 and .55-(.55-.30)*at/.4 or .30+(.62-.30)*(at-.4)/.6 end
local children={}
for band=0,W.veil_bands-1 do
  -- Even edges land on whole physical pixels at scale 1, 1.5 and 2, so the bands leave no antialiased seams.
  local function edge(index) return index==W.veil_bands and height or 2*math.floor(height*index/W.veil_bands/2) end
  local top,bottom=edge(band),edge(band+1)
  children[#children+1]=ui.Box{id="veil-"..band,position="absolute",left=0,top=top,width=width,height=bottom-top,radius=0,fill={6,7,10,math.floor(255*veil_alpha((band+.5)/W.veil_bands))}}
end
children[#children+1]=ui.Box{id="strand",position="absolute",left=0,top=-(1-entrance)*W.drop*s,width=width,height=height,fill="transparent",table.unpack(strand_nodes)}

if selected then
  local name_top=centre.y+W.name_top*s
  children[#children+1]=ui.Text{id="selected-title",position="absolute",left=0,top=name_top,width=width,height=W.name_height*s,
    text=selected.title or "",font_size=W.name_font*s,font_weight=600,text_align="center",color=W.ink,
    text_shadow_blur=18,text_shadow_y=2,text_shadow_color={0,0,0,115}}
  local meta=string.format("%02d / %d",selection_index,count)
  if resolution~="" and resolution_id==selected.id then meta=meta.." · "..resolution end
  local meta_font=W.meta_font*s
  local meta_row={ui.Text{id="selection-count",width=ui.text_width(meta,"JetBrains Mono NF",meta_font)+2,height=W.meta_height*s,text=meta,
    family="JetBrains Mono NF",font_size=meta_font,color=W.ink_soft}}
  local dots={}
  local colours=type(selected.palette)=="table" and selected.palette.colours or nil
  for _,role in ipairs(W.palette_roles) do
    local r,g,b=string.match(type(colours)=="table" and tostring(colours[role] or "") or "","#(%x%x)(%x%x)(%x%x)")
    if r then
      dots[#dots+1]=ui.Box{id="dot-"..role,width=W.dot*s,height=W.dot*s,shrink=0,radius=W.dot*s/2,
        fill={tonumber(r,16),tonumber(g,16),tonumber(b,16),255},border_width=1,border_color=W.dot_edge}
    end
  end
  if #dots>0 then
    meta_row[#meta_row+1]=ui.Row{id="palette-dots",gap=W.dot_gap*s,height=W.dot*s,align="center",table.unpack(dots)}
  end
  children[#children+1]=ui.Row{id="selection-meta",position="absolute",left=0,top=centre.y+W.meta_top*s,width=width,height=W.meta_height*s,
    gap=W.meta_gap*s,align="center",justify="center",table.unpack(meta_row)}
  local nav=W.nav_size*s
  local nav_top=name_top+(W.name_height*s-nav)/2
  children[#children+1]=glyph_button("previous",ui.tr("bead.dashboard.wallpaper-previous"),"chevron_left",nav,W.nav_font*s,W.nav_fill,
    "extras.prev",true,centre.x-W.nav_offset*s-nav/2,nav_top)
  children[#children+1]=glyph_button("next",ui.tr("bead.dashboard.wallpaper-next"),"chevron_right",nav,W.nav_font*s,W.nav_fill,
    "extras.next",true,centre.x+W.nav_offset*s-nav/2,nav_top)
end

-- One bottom pill: Static | Dynamic, a divider, search, (pause while a video plays), shuffle.
local button=W.button_size*s
local pill_items={
  ui.Row{id="media",height=button,gap=0,shrink=0,align="center",
    toggle("static","Static",kind=="static","wallpaper:kind:static",W.toggle_static),
    toggle("dynamic","Dynamic",kind=="dynamic","wallpaper:kind:dynamic",W.toggle_dynamic)},
  ui.Box{id="media-divider",width=1,height=W.divider_height*s,shrink=0,fill=W.divider_fill},
  ui.Box{id="search-field",width=W.search_width*s,height=button,shrink=0,fill="transparent",
    kit.TextInput{id="search",position="absolute",left=0,top=0,width=W.search_width*s,height=button,
      focus_ring=false,text=ui.bind("extras.query"),placeholder="Search wallpapers",
      padding_left=(W.search_icon+22)*s,padding_right=12*s,padding_top=(button-kit.tokens.text.body*4/3*1.4)/2,padding_bottom=(button-kit.tokens.text.body*4/3*1.4)/2,
      radius=button/2,font_size=kit.tokens.text.body*4/3,fill=W.search_fill,
      border_width=search_focused and 1 or 0,border_color="outline"},
    ui.Text{id="search-icon",position="absolute",left=14*s,top=(button-W.search_icon*s*1.15)/2,width=W.search_icon*s,height=W.search_icon*s*1.15,
      text="search",family="Material Symbols Rounded",font_size=W.search_icon*s,color=W.ink_soft}}}
if can_pause then
  pill_items[#pill_items+1]=glyph_button("pause",ui.tr(paused and "bead.dashboard.wallpaper-resume" or "bead.dashboard.wallpaper-pause"),
    paused and "play_arrow" or "pause",button,W.button_font*s,"transparent","extras.do","wallpaper:pause")
end
pill_items[#pill_items+1]=glyph_button("shuffle",ui.tr("bead.dashboard.wallpaper-shuffle"),"shuffle",button,W.button_font*s,"transparent","extras.shuffle",true)
local pill_width=W.pill_padding*2*s+W.toggle_static*s+W.toggle_dynamic*s+1+W.search_width*s+button*(can_pause and 2 or 1)+W.pill_gap*s*(can_pause and 4 or 3)
local pill_height=W.pill_height*s
children[#children+1]=ui.Box{id="wallpaper-controls",position="absolute",left=(width-pill_width)/2,top=height-W.pill_bottom*s-pill_height,
  width=pill_width,height=pill_height,radius=pill_height/2,fill=W.pill_fill,
  ui.Row{width=pill_width,height=pill_height,padding_left=W.pill_padding*s,padding_right=W.pill_padding*s,gap=W.pill_gap*s,align="center",table.unpack(pill_items)}}

children[#children+1]=ui.Text{id="empty",position="absolute",left=24,top=height/2+40*s,width=width-48,height=W.empty_height,
  text=ui.bind("extras.empty_message"),visible=count==0 and not ui.resolve_for_check(ui.bind("extras.status_visible")),
  font_size=T.panel.empty_font,text_align="center",color=W.ink}
children[#children+1]=ui.Text{id="status",position="absolute",left=24,top=height/2+72*s,width=width-48,height=W.status_height,
  text=ui.bind("extras.status_message"),visible=ui.bind("extras.status_visible"),font_size=T.panel.empty_font,
  text_align="center",color="onSurfaceVariant"}

-- The shared panel profile: a transparent root whose body grows out of the bar edge; a click on empty
-- space closes the picker like Esc (the wallpaper goes back).
return kit.Panel{id="extras",edge="top",open=ui.bind("panel.open"),width=width,height=height,shadow=false,reveal=true,fade=false,
  radius=0,fill="transparent",padding=0,gap=0,body="body",on_activate=ui.action("close"),aria_label="Close wallpaper picker",
  ui.Box{id="body",position="absolute",left=0,top=0,width=width,height=height,fill="transparent",join_edge="top",table.unpack(children)}}
