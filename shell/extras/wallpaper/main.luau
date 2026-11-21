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
local search_focused=ui.resolve_for_check(ui.bind("bead.focused"))=="extras.search"
local rows=ui.resolve_for_check(ui.bind("extras.filtered")) or {}
local count=#rows
local selected
for _,item in ipairs(rows) do if item.selected then selected=item; break end end
local selection_index=selected and selected.selection_index or 1
local resolution=ui.resolve_for_check(ui.bind("extras.wallpaper.resolution")) or ""
local resolution_id=ui.resolve_for_check(ui.bind("extras.wallpaper.resolution_id")) or ""
local position=ui.resolve_for_check(ui.bind("extras.wallpaper.position")) or 1
if math.floor(position-1)%math.max(1,count)+1~=selection_index then position=selection_index end
local animation_query=tostring(ui.resolve_for_check(ui.bind("extras.query")) or "")
local animation_key=2166136261
for index=1,#animation_query do animation_key=(animation_key*33+string.byte(animation_query,index))%2147483647 end
-- The entrance (owner, round 4: no staged overlay): everything arrives together in one out-cubic, opacity and a small
-- drop; the exit is the same in reverse, shorter (the panel's `return_duration` keeps the surface until it ends).
-- Both run on `ui.animated` markers of two Boxes (B0349 group opacity), so those frames patch and bake with no Luau.
-- `extras.wallpaper.entry_ms` pins the entrance for goldens (>= 0).
local opening=ui.resolve_for_check(ui.bind("panel.open"))~=false
local pinned=ui.resolve_for_check(ui.bind("extras.wallpaper.entry_ms")) or -1 -- only goldens pin it (>= 0)
local appear=pinned>=0 and 1-(1-math.min(1,pinned/W.open_ms))^3 or nil
local entry_s=(opening and W.open_ms or W.close_ms)/1000

-- One continuous value drives the whole strand: pearls slide along the thread to a fractional slot.
-- A held key or a fast scroll (`glide`) follows its moving target at a steady pace instead of restarting the
-- out-quint for every repeat; a single tap keeps the full 500 ms out-quint.
local glide=ui.resolve_for_check(ui.bind("extras.wallpaper.glide"))==true
-- A new visit starts at rest on its pearl (its own animation key), and a closing strand does not glide while it leaves.
local visit=ui.resolve_for_check(ui.bind("extras.wallpaper.visit")) or 0
local strand=ui.animate("wallpaper-strand:"..tostring(visit)..":"..tostring(count)..":"..tostring(animation_key),position,not opening and 0 or glide and W.glide_duration or W.move_duration,glide and "linear" or "out-quint")
local s=math.min(width/W.art_width,height/W.art_height)
local centre={x=width/2,y=height/2}
local gap=W.pearl_gap*s
local drop=W.entry_drop*s -- the small drop: everything comes down into place
local fade_in=appear or ui.animated("wallpaper-strand-fade",opening and 1 or 0,entry_s,"out-cubic",0)
local lift=appear and (1-appear)*drop or ui.animated("wallpaper-strand-lift",opening and 0 or drop,entry_s,"out-cubic",drop)

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
  -- B0328: every image is baked once at its largest slot size and the draw scales it, so sliding never re-bakes.
  local bake=math.abs(pearl.offset)<=2.5 and size_at(0) or size_at(3)
  local item=pearl.item
  -- The outer pair rests at 50 % (offset 4 -> 5), also while the strand moves.
  local alpha=(1-.5*math.max(0,math.min(1,math.abs(pearl.offset)-4)))
  local size,cy=pearl.size,pearl.y
  local near=math.max(0,1-math.abs(pearl.offset))
  local edge=math.max(1,s)
  local path=item.thumbnail
  local kids={}
  -- The shadow and the sheen keep one tint and fade through opacity: a tint is part of the baked image's key, so a
  -- tint that changed every frame re-baked both images on every frame of a glide or the drop.
  if math.abs(pearl.offset)<=2.5 then
    kids[#kids+1]=ui.Image{id="shadow",position="absolute",left=-size*.12,top=-size*.04,width=size*1.24,height=size*1.24,
      path=W.glow,bake_size={w=bake*1.24,h=bake*1.24},tint={0,0,0,110},opacity=alpha*math.min(1,size/(W.pearl_sizes[1]*s)+.35)}
  end
  if path and path~="" then
    kids[#kids+1]=ui.Image{id="thumbnail",position="absolute",left=0,top=0,width=size,height=size,
      icon=path,bake_size={w=bake,h=bake},image_fit="cover",clip_polygon=DISC,opacity=alpha,aria_label=item.title or "Wallpaper"}
  else
    kids[#kids+1]=ui.Box{id="missing-thumbnail",position="absolute",left=0,top=0,width=size,height=size,radius=size/2,fill="surfaceContainerHigh"}
    kids[#kids+1]=ui.Text{id="missing-icon",position="absolute",left=0,top=size/2-W.thumbnail_fallback_icon*s/2,width=size,height=W.thumbnail_fallback_icon*s,
      text=item.kind=="video" and "movie" or "image",family="Material Symbols Rounded",font_size=W.thumbnail_fallback_icon*s,
      color="onSurfaceVariant",text_align="center"}
  end
  -- The prototype's inset bottom shade (inset 0 -14px 26px rgba(0,0,0,.34)), under the rim.
  if math.abs(pearl.offset)<=2.5 then
    kids[#kids+1]=ui.Image{id="shade",position="absolute",left=0,top=0,width=size,height=size,path=W.shade,bake_size={w=bake,h=bake},opacity=alpha}
  end
  kids[#kids+1]=ui.Box{id="edge",position="absolute",left=0,top=0,width=size,height=size,radius=size/2,fill="transparent",
    border_width=edge,border_color=W.pearl_edge}
  if near>.3 then
    kids[#kids+1]=ui.Image{id="sheen",position="absolute",left=0,top=0,width=size,height=size,path=W.sheen,bake_size={w=bake,h=bake},
      tint=W.pearl_sheen,opacity=near*alpha}
  end
  -- The chosen pearl keeps the pick; any other pearl only moves the strand.
  local request=(math.abs(pearl.offset)<.5 and "wallpaper:commit:" or "wallpaper:select:")..tostring(item.id)
  return ui.Button{id="pearl-"..tostring(item.selection_index),position="absolute",left=pearl.x-size/2,top=cy-size/2,
    width=size,height=size,padding=0,radius=size/2,fill="transparent",hover_fill="transparent",
    activate_on_release=true,focus_on_pointer=false,on_activate=ui.action("extras.do",request),
    aria_label=(item.title or "Wallpaper")..(item.active and ", current" or ""),
    ui.Box{width=size,height=size,fill="transparent",table.unpack(kids)}}
end

-- Silk: a dark shadow stroke under the thread, each ONE polyline sampled from the catenary (a point every ~3.5 px,
-- Bead's cap is 256 points, still within 0.01 px of the curve at 2880 px), so the curve has no joints. Bead strokes
-- one colour per polyline, so the thread is the wallpaper's `primary` role. The box spans the panel width and the sag
-- height: a wide, shallow strip, which keeps the CPU renderer's repair scratch small.
local function silk(id,colour,stroke)
  local steps=math.min(255,math.ceil(width/3.5))
  local top,bottom,curve=math.huge,-math.huge,{}
  for step=0,steps do
    local x=width*step/steps
    local y=base-a*math.cosh((x-centre.x)/a)
    curve[#curve+1]={x,y}
    top,bottom=math.min(top,y),math.max(bottom,y)
  end
  local pad=stroke*s+2
  local left,box_width,box_height=-pad,width+2*pad,bottom-top+2*pad
  local points={}
  for _,point in ipairs(curve) do points[#points+1]={(point[1]-left)/box_width,(point[2]-top+pad)/box_height} end
  return ui.Polyline{id=id,position="absolute",left=left,top=top-pad,width=box_width,height=box_height,points=points,stroke_width=stroke*s,color=colour}
end
local strand_nodes={silk("thread-shadow",W.silk_shadow,W.silk_stroke+2),silk("thread","primary",W.silk_stroke)}

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
    border_width=type(fill)=="table" and fill[4]>0 and 1 or nil,border_color=W.nav_ring,
    ui.Row{width=size,height=size,align="center",justify="center",
      ui.Text{text=glyph,width=font,height=font*1.15,family="Material Symbols Rounded",font_size=font,color=W.ink}}}
  if left then props.position,props.left,props.top="absolute",left,top end
  return kit.Button(props)
end

-- Veil: only what the name, caption and pill need to read (owner, round 4: no dim that makes Strand an overlay):
-- nothing over the upper part, then flat translucent bands to `veil_max` at the bottom. A stretched SVG would be
-- rasterised at the full panel size, which overruns Bead's retained raster budget at 2880x1800.
local function veil_alpha(at) return W.veil_max*math.max(0,at-W.veil_from)/(1-W.veil_from) end
local veil={}
local children={}
for band=0,W.veil_bands-1 do
  -- Even edges land on whole physical pixels at scale 1, 1.5 and 2, so the bands leave no antialiased seams.
  local function edge(index) return index==W.veil_bands and height or 2*math.floor(height*index/W.veil_bands/2) end
  local top,bottom=edge(band),edge(band+1)
  veil[#veil+1]=ui.Box{id="veil-"..band,position="absolute",left=0,top=top,width=width,height=bottom-top,radius=0,fill={6,7,10,math.floor(255*veil_alpha((band+.5)/W.veil_bands))}}
end
-- Wheel or trackpad: at most one step per 60 ms (Bead drops the rest); extras.rs lands it 240 ms after the last step.
children[#children+1]=ui.Box{id="strand",position="absolute",left=0,top=0,width=width,height=height,fill="transparent",
  scroll=ui.action("strand_wheel"),scroll_steps={min_interval_ms=60},table.unpack(strand_nodes)}

if selected then
  local name_top=centre.y+W.name_top*s
  -- A long name ends in an ellipsis inside the span between the arrows (they sit a fixed gap outside it).
  local name_max,name_size=2*(W.nav_offset-W.nav_size/2-W.nav_gap)*s,W.name_font*s
  local title=selected.title or ""
  if ui.text_width(title,ui.default_font_family,name_size)>name_max then
    local chars=utf8.len(title) or #title
    repeat
      chars=chars-1
      title=string.sub(selected.title,1,(utf8.offset(selected.title,chars+1) or #selected.title+1)-1).."…"
    until chars<=1 or ui.text_width(title,ui.default_font_family,name_size)<=name_max
  end
  children[#children+1]=ui.Text{id="selected-title",position="absolute",left=(width-name_max)/2,top=name_top,width=name_max,height=W.name_height*s,
    text=title,font_size=W.name_font*s,font_weight=300,text_align="center",color=W.ink,
    text_shadow_blur=18,text_shadow_y=2,text_shadow_color={0,0,0,115}}
  -- The prototype's caption: "3 of 47, 3840 x 2560" for a still, "3 of 47, video, 24 s loop, plays muted" for a video;
  -- "3 of 5 matches" while a search narrows the strand.
  local meta=animation_query~="" and ui.tr("bead.dashboard.wallpaper-meta-matches",{index=selection_index,count=count})
    or ui.tr("bead.dashboard.wallpaper-meta",{index=selection_index,total=count})
  if selected.kind=="video" then meta=ui.tr("bead.dashboard.wallpaper-meta-video",{meta=meta})
  elseif resolution~="" and resolution_id==selected.id then meta=ui.tr("bead.dashboard.wallpaper-meta-size",{meta=meta,size=resolution}) end
  local meta_font=W.meta_font*s
  local meta_row={ui.Text{id="selection-count",width=ui.text_width(meta,ui.default_font_family,meta_font)+12*s,height=W.meta_height*s,text=meta,
    font_weight=500,font_size=meta_font,color=W.ink_soft}}
  local dots={}
  local colours=selected.swatch or (type(selected.palette)=="table" and selected.palette.colours or nil)
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

-- One pill, centred under the name and meta: search and shuffle (the prototype has no pause).
local button=W.button_size*s
local pill_items={
  ui.Box{id="search-field",width=W.search_width*s,height=button,shrink=0,fill="transparent",
    kit.TextInput{id="search",position="absolute",left=0,top=0,width=W.search_width*s,height=button,
      focus_ring=false,text=ui.bind("extras.query"),placeholder="Search wallpapers",
      padding_left=(W.search_icon+19)*s,padding_right=12*s,padding_top=(button-kit.tokens.text.body*4/3*1.4)/2,padding_bottom=(button-kit.tokens.text.body*4/3*1.4)/2,
      radius=button/2,font_size=kit.tokens.text.body*4/3,fill=W.search_fill,
      border_width=search_focused and 1 or 0,border_color="outline"},
    ui.Text{id="search-icon",position="absolute",left=10*s,top=(button-W.search_icon*s*1.15)/2,width=W.search_icon*s,height=W.search_icon*s*1.15,
      text="search",family="Material Symbols Rounded",font_size=W.search_icon*s,color=W.ink_soft}}}
pill_items[#pill_items+1]=glyph_button("shuffle",ui.tr("bead.dashboard.wallpaper-shuffle"),"shuffle",button,W.button_font*s,"transparent","extras.shuffle",true)
local pill_width=W.pill_padding*2*s+W.search_width*s+button+W.pill_gap*s
local pill_height=W.pill_height*s
children[#children+1]=ui.Box{id="wallpaper-controls",position="absolute",left=(width-pill_width)/2,top=centre.y+W.pill_top*s,
  width=pill_width,height=pill_height,radius=pill_height/2,fill=W.pill_fill,
  ui.Row{width=pill_width,height=pill_height,padding_left=W.pill_padding*s,padding_right=W.pill_padding*s,gap=W.pill_gap*s,align="center",table.unpack(pill_items)}}

children[#children+1]=ui.Text{id="empty",position="absolute",left=24,top=height/2+40*s,width=width-48,height=W.empty_height,
  text=ui.bind("extras.empty_message"),visible=count==0 and not ui.resolve_for_check(ui.bind("extras.status_visible")),
  font_size=T.panel.empty_font,text_align="center",color=W.ink}
children[#children+1]=ui.Text{id="status",position="absolute",left=24,top=height/2+72*s,width=width-48,height=W.status_height,
  text=ui.bind("extras.status_message"),visible=ui.bind("extras.status_visible"),font_size=T.panel.empty_font,
  text_align="center",color="onSurfaceVariant"}

-- A transparent full-size root that only keeps the surface alive for the storyboard (no reveal, no fade, no
-- slide: the shared profile would fight it); a click on empty
-- space closes the picker like Esc (the wallpaper goes back). No open motion either (`duration=0`): Bead keeps a
-- panel on its CPU renderer while the panel motion runs, which at full screen held the strand's first GL frame
-- back ~200 ms behind 10-20 ms CPU frames. `palette_tween=false`: Strand's look takes nothing from the palette, so a
-- wallpaper's palette tween re-staged the whole strand ~30 times per move; it now takes the tween's final palette only.
-- The invisible edge makes the root the scene's first box, which the shell places as the panel body (otherwise the
-- first veil band or pearl was taken for it). `view.frame=false` (behavior.luau) keeps the frame art out of Strand.
return kit.Panel{id="extras",edge="top",open=ui.bind("panel.open"),width=width,height=height,shadow=false,reveal=false,fade=false,slide_distance=0,
  duration=0,return_duration=W.close_ms/1000,palette_tween=false,key_repeat={delay=260,interval=95,min_interval=62,ramp_ms=800}, -- the held-arrow glide clock (spec v5f)
  radius=0,fill="transparent",border_width=1,border_color="transparent",padding=0,gap=0,body="body",on_activate=ui.action("close"),aria_label="Close wallpaper picker",
  ui.Box{id="body",position="absolute",left=0,top=0,width=width,height=height,fill="transparent",join_edge="top",
    ui.Box{id="veil",position="absolute",left=0,top=0,width=width,height=height,opacity=fade_in,table.unpack(veil)},
    ui.Box{id="appear",position="absolute",left=0,top=0,width=width,height=height,opacity=fade_in,translate_y=lift,table.unpack(children)}}}
