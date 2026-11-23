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
-- B3: the strand is built once per move and everything on it follows this driver on keyframe tracks, so the frames
-- of a move patch the scene (no Luau, no layout). Each part is laid out where it rests at `to`.
local drive=ui.animated("wallpaper-strand:"..tostring(visit)..":"..tostring(count)..":"..tostring(animation_key),position,not opening and 0 or glide and W.glide_duration or W.move_duration,glide and "linear" or "out-quint")
local from,to=ui.range(drive)
-- shortcut: a jump of more than 24 slots (a shuffle over 48+ wallpapers) holds still until its last 24, which keeps
-- the move's build within Bead's markup budget and track points; key the far part sparsely if that ever shows.
local low,high=math.max(math.min(from,to),to-24),math.min(math.max(from,to),to+24)
-- Where the move starts on screen (a held far part starts at its last 24).
local start=math.clamp(from,low,high)
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

-- A pearl `o` slots from the driver is drawn within reach, each wallpaper once around the loop.
local reach_max=math.min(W.visible_reach,count/2)
-- Parts and keys share one grid, so offsets repeat: work each one out once.
local function memo(f) local known={} return function(o) local p=known[o] if not p then p=f(o); known[o]=p end return p end end
-- A pearl at offset `o` by the old per-frame formulas; out of reach it keeps the last place it was drawn at, unseen.
local pose=memo(function(o)
  local seen=o>=-count/2 and o<count/2 and math.abs(o)<=W.visible_reach
  o=math.clamp(o,-reach_max,reach_max)
  local d=math.abs(o)
  local arc,size=math.sign(o)*reach(d),size_at(d)
  local x,y=point_at(arc)
  -- The outer pair rests at 50 % (offset 4 -> 5), also while the strand moves.
  local alpha=seen and (1-.5*math.max(0,math.min(1,d-4))) or 0
  local near=math.max(0,1-d)
  return {x=x,y=y,arc=arc,size=size,shown=seen and 1 or 0,thumbnail=alpha,shade=d<=2.5 and alpha or 0,
    shadow=d<=2.5 and alpha*math.min(1,size/(W.pearl_sizes[1]*s)+.35) or 0,sheen=near>.3 and near*alpha or 0}
end)

-- The driver values a part needs while its offset i-d is within [omin,omax]: the move's ends, a key every 1/fine
-- slot and both sides of each `cuts` offset (a step: something appears or goes) within 6 slots of `to`, whole slots
-- (every kink of the formulas) farther out, where it moves fast (Bead takes at most 128 keys), and one key just
-- outside the window, where the part is gone. At rest (low == high) that is the one key `to`.
local EPS=1e-6
local function keys_for(i,omin,omax,fine,cuts)
  local a,b=math.max(low,i-omax-EPS),math.min(high,i-omin+EPS)
  local keys={a,b}
  for k=math.floor(a),math.floor(b) do
    for j=0,fine-1 do
      local key=k+j/fine
      if key>a and key<b and (j==0 or math.abs(key-to)<6) then keys[#keys+1]=key end
    end
  end
  for _,cut in ipairs(cuts) do
    for _,key in ipairs({i-cut-EPS,i-cut+EPS}) do if key>a and key<b and math.abs(key-to)<6 then keys[#keys+1]=key end end
  end
  table.sort(keys)
  local out={}
  for _,key in ipairs(keys) do if #out==0 or key>out[#out] then out[#out+1]=key end end
  return out
end
local function sample(keys,at) local poses={} for n,key in ipairs(keys) do poses[n]=at(key) end return poses end
-- One prop over the move: its value at each key, a plain number at rest.
local function along(keys,poses,value)
  local values={}
  for n,p in ipairs(poses) do values[n]=value(p) end
  return #keys==1 and values[1] or ui.track(drive,keys,values)
end

-- Pearl `i` (unwrapped slot): z-order, images and the action are fixed for the move, so its frames only patch.
local function pearl_node(i)
  local item=rows[(i-1)%count+1]
  local keys=keys_for(i,-reach_max,reach_max,8,{-reach_max,reach_max,-2.5,2.5,-.7,.7})
  local poses=sample(keys,function(key) return pose(i-key) end)
  local function ever(name) for _,p in ipairs(poses) do if p[name]>0 then return true end end return false end
  local function fade(name) return along(keys,poses,function(p) return p[name] end) end
  local rest=pose(i-to)
  local size=rest.size
  -- B0328: every image is baked once at its largest slot size and the draw scales it, so sliding never re-bakes.
  -- Only the pearls the move starts or lands on near the centre bake at full size: one a shuffle only carries past it
  -- stays small (all of them at full size were ~29 x 1.2 MB in a shuffle's first frame, over Bead's texture cap).
  local bake=ever("shadow") and math.min(math.abs(i-to),math.abs(i-start))<=2.5 and size_at(0) or size_at(3)
  local edge=math.max(1,s)
  local path=item.thumbnail
  local kids={}
  -- The shadow and the sheen keep one tint and fade through opacity: a tint is part of the baked image's key, so a
  -- tint that changed every frame re-baked both images on every frame of a glide or the drop.
  if ever("shadow") then
    kids[#kids+1]=ui.Image{id="shadow",position="absolute",left=-size*.12,top=-size*.04,width=size*1.24,height=size*1.24,
      path=W.glow,bake_size={w=bake*1.24,h=bake*1.24},tint={0,0,0,110},opacity=fade("shadow")}
  end
  if path and path~="" then
    kids[#kids+1]=ui.Image{id="thumbnail",position="absolute",left=0,top=0,width=size,height=size,
      icon=path,bake_size={w=bake,h=bake},image_fit="cover",clip_polygon=DISC,opacity=fade("thumbnail"),aria_label=item.title or "Wallpaper"}
  else
    kids[#kids+1]=ui.Box{id="missing-thumbnail",position="absolute",left=0,top=0,width=size,height=size,radius=size/2,fill="surfaceContainerHigh"}
    kids[#kids+1]=ui.Text{id="missing-icon",position="absolute",left=0,top=size/2-W.thumbnail_fallback_icon*s/2,width=size,height=W.thumbnail_fallback_icon*s,
      text=item.kind=="video" and "movie" or "image",family="Material Symbols Rounded",font_size=W.thumbnail_fallback_icon*s,
      color="onSurfaceVariant",text_align="center"}
  end
  -- The prototype's inset bottom shade (inset 0 -14px 26px rgba(0,0,0,.34)), under the rim.
  if ever("shade") then
    kids[#kids+1]=ui.Image{id="shade",position="absolute",left=0,top=0,width=size,height=size,path=W.shade,bake_size={w=bake,h=bake},opacity=fade("shade")}
  end
  kids[#kids+1]=ui.Box{id="edge",position="absolute",left=0,top=0,width=size,height=size,radius=size/2,fill="transparent",
    border_width=edge,border_color=W.pearl_edge}
  if ever("sheen") then
    kids[#kids+1]=ui.Image{id="sheen",position="absolute",left=0,top=0,width=size,height=size,path=W.sheen,bake_size={w=bake,h=bake},
      tint=W.pearl_sheen,opacity=fade("sheen")}
  end
  local props={id="pearl-"..tostring(i),position="absolute",left=rest.x-size/2,top=rest.y-size/2,width=size,height=size,
    translate_x=along(keys,poses,function(p) return p.x-rest.x end),translate_y=along(keys,poses,function(p) return p.y-rest.y end),
    scale=along(keys,poses,function(p) return p.size/size end),opacity=fade("shown"),
    ui.Box{width=size,height=size,fill="transparent",table.unpack(kids)}}
  -- Gone at `to`: drawn while it leaves, never hit (a zero-opacity button still takes clicks).
  if rest.shown==0 then return ui.Box(props) end
  -- The chosen pearl keeps the pick; any other pearl only moves the strand.
  local request=(math.abs(i-to)<.5 and "wallpaper:commit:" or "wallpaper:select:")..tostring(item.id)
  props.padding,props.radius,props.fill,props.hover_fill=0,size/2,"transparent","transparent"
  props.activate_on_release,props.focus_on_pointer,props.on_activate=true,false,ui.action("extras.do",request)
  props.aria_label=(item.title or "Wallpaper")..(item.active and ", current" or "")
  return ui.Button(props)
end

-- The knot between pearls i and i+1 sits halfway along the silk between their facing edges.
local knot=math.max(3,W.knot*s)
local knot_pose=memo(function(o)
  local left,right=pose(o),pose(o+1)
  local x,y=point_at(((left.arc+left.size/2)+(right.arc-right.size/2))/2)
  return {x=x,y=y,shown=left.shown*right.shown}
end)
local function knot_node(i)
  local keys=keys_for(i,-reach_max,reach_max-1,8,{-reach_max,reach_max-1})
  local poses=sample(keys,function(key) return knot_pose(i-key) end)
  local rest=knot_pose(i-to)
  return ui.Box{id="knot-"..tostring(i),position="absolute",left=rest.x-knot/2,top=rest.y-knot/2,width=knot,height=knot,radius=knot/2,fill=W.knot_fill,
    translate_x=along(keys,poses,function(p) return p.x-rest.x end),translate_y=along(keys,poses,function(p) return p.y-rest.y end),
    opacity=along(keys,poses,function(p) return p.shown end)}
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
local strand_nodes={silk("thread-shadow",W.silk_shadow,W.silk_stroke+3),silk("thread","primary",W.silk_stroke)}

-- Every pearl drawn at some point of the move (at rest: those within reach), the knots between them, then the
-- pearls farthest from `to` first, so the chosen one draws on top.
local placed={}
if count>0 then
  for i=math.ceil(low-reach_max),math.floor(high+reach_max) do
    if i-high<count/2 then placed[#placed+1]=i end
  end
end
if count>1 then for n=1,#placed-1 do strand_nodes[#strand_nodes+1]=knot_node(placed[n]) end end
table.sort(placed,function(left,right) return math.abs(left-to)>math.abs(right-to) end)
for _,i in ipairs(placed) do strand_nodes[#strand_nodes+1]=pearl_node(i) end

-- A thin conic ring (five arcs from pink round to cream) around the lowest point, one Box that scales and fades.
-- Between two pearls the ring has nothing to hold: it fades, then comes back around the next one.
local stroke=math.max(1,W.ring_stroke*s)
local function ring_pose(d)
  local nearest=math.abs(d-math.floor(d+.5))
  return {size=size_at(nearest)+2*(W.ring_gap+W.ring_stroke)*s,alpha=math.floor(255*math.max(0,math.min(1,1-nearest*1.6)))/255}
end
if count>0 then
  -- The ring's size and alpha are linear between half slots.
  local keys=keys_for(0,-math.huge,math.huge,2,{})
  local poses=sample(keys,ring_pose)
  local rest=ring_pose(to)
  local ring=rest.size-stroke
  local arcs={}
  for index,colour in ipairs(W.silk) do
    arcs[index]=ui.Arc{id="ring-"..index,position="absolute",left=0,top=0,width=ring,height=ring,start_angle=-160+(index-1)*72,sweep_angle=72,
      stroke_width=stroke,round_caps=false,color={colour[1],colour[2],colour[3],255}}
  end
  strand_nodes[#strand_nodes+1]=ui.Box{id="ring",position="absolute",left=centre.x-rest.size/2+stroke/2,top=centre.y-rest.size/2+stroke/2,
    width=ring,height=ring,scale=along(keys,poses,function(p) return (p.size-stroke)/ring end),
    opacity=along(keys,poses,function(p) return p.alpha end),table.unpack(arcs)}
end

local function glyph_button(id,label,glyph,size,font,fill,action,request,left,top,ink)
  local props={id=id,aria_label=label,width=size,height=size,shrink=0,padding=0,radius=size/2,fill=fill,hover_fill=fill,
    activate_on_release=true,focus_on_pointer=false,on_activate=ui.action(action,request),
    border_width=type(fill)=="table" and fill[4]>0 and 1 or nil,border_color=W.nav_ring,
    ui.Row{width=size,height=size,align="center",justify="center",
      ui.Text{text=glyph,width=font,height=font*1.15,family="Material Symbols Rounded",font_size=font,color=ink or W.ink}}}
  if left then props.position,props.left,props.top="absolute",left,top end
  return kit.Button(props)
end

-- Veil: only what the name, caption and pill need to read (owner, round 4: no dim that makes Strand an overlay):
-- nothing over the upper part, then one baked ramp image (alpha 0 at `veil_from` of the height, `veil_max` at the bottom).
-- A ramp, not stacked flat bands: those showed a step per band on light wallpapers. Bead's GL path bakes it at 8x256 and
-- draws it stretched; the CPU path rasters it at its rect (about 11 MB at 2880x1800, within the retained budget).
local veil_top=math.floor(height*W.veil_from)
local veil={ui.Image{id="veil-ramp",path=W.veil,image_fit="fill",bake_size={w=8,h=256},position="absolute",left=0,top=veil_top,width=width,height=height-veil_top,opacity=W.veil_max}}
local children={}
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
      radius=button/2,font_size=kit.tokens.text.body*4/3,fill=ui.alpha(W.search_fill,W.search_alpha),
      border_width=search_focused and 1 or 0,border_color="outline"},
    ui.Text{id="search-icon",position="absolute",left=10*s,top=(button-W.search_icon*s*1.15)/2,width=W.search_icon*s,height=W.search_icon*s*1.15,
      text="search",family="Material Symbols Rounded",font_size=W.search_icon*s,color="onSurfaceVariant"}}}
pill_items[#pill_items+1]=glyph_button("shuffle",ui.tr("bead.dashboard.wallpaper-shuffle"),"shuffle",button,W.button_font*s,"transparent","extras.shuffle",true,nil,nil,"onSurface")
local pill_width=W.pill_padding*2*s+W.search_width*s+button+W.pill_gap*s
local pill_height=W.pill_height*s
children[#children+1]=ui.Box{id="wallpaper-controls",position="absolute",left=(width-pill_width)/2,top=centre.y+W.pill_top*s,
  width=pill_width,height=pill_height,radius=pill_height/2,fill=ui.alpha(W.pill_fill,W.pill_alpha),
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
-- back ~200 ms behind 10-20 ms CPU frames. Strand takes every step of a landing's palette tween (its thread is the
-- `primary` role): with only the last step it snapped ~500 ms after the bar had eased, a second colour change.
-- The invisible edge makes the root the scene's first box, which the shell places as the panel body (otherwise the
-- first veil band or pearl was taken for it). `view.frame=false` (behavior.luau) keeps the frame art out of Strand.
return kit.Panel{id="extras",edge="top",open=ui.bind("panel.open"),width=width,height=height,shadow=false,reveal=false,fade=false,slide_distance=0,
  duration=0,return_duration=W.close_ms/1000,key_repeat={delay=260,interval=95,min_interval=62,ramp_ms=800}, -- the held-arrow glide clock (spec v5f)
  radius=0,fill="transparent",border_width=1,border_color="transparent",padding=0,gap=0,body="body",on_activate=ui.action("close"),aria_label="Close wallpaper picker",
  ui.Box{id="body",position="absolute",left=0,top=0,width=width,height=height,fill="transparent",join_edge="top",
    ui.Box{id="veil",position="absolute",left=0,top=0,width=width,height=height,opacity=fade_in,table.unpack(veil)},
    ui.Box{id="appear",position="absolute",left=0,top=0,width=width,height=height,opacity=fade_in,translate_y=lift,table.unpack(children)}}}
