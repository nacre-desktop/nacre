-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local c=require("../widgets/context")
local widgets=require("../widgets/registry")
local saved=ui.resolve_for_check(ui.bind("widgets.layout"))
local draft=ui.resolve_for_check(ui.bind("widgets.draft"))
local drag_layout=ui.resolve_for_check(ui.bind("widgets.drag.layout"))
local drag_phase=ui.resolve_for_check(ui.bind("widgets.drag.phase"))
local dragging=ui.resolve_for_check(ui.bind("widgets.drag.active"))==true
    and (drag_phase=="dragging" or drag_phase=="settle")
local editing=ui.resolve_for_check(ui.bind("widgets.editing"))
local width=ui.resolve_for_check(ui.bind("bead.viewport.width"))
local selected_modules=ui.resolve_for_check(ui.bind("bar.modules"))
local document=(dragging and drag_layout and drag_layout[1]) or (editing and draft[1]) or saved[1]
    or widgets.default(width)
if not dragging and not editing then
    document=widgets.default(width,selected_modules,document)
end
local canonical=widgets.normalise(document)
document=widgets.project(canonical,ui.resolve_for_check(ui.bind("bead.viewport.width")))
local render=widgets.renderer(document)
local layout=document.hosts.top_bar
local regions=widgets.bar_regions(layout)
local flow,placed,extras_right=widgets.bar_layout(layout,render,width)
-- The recording stop button (only while recording) and the tray come before the right strip. When the right-hand
-- widgets are placed (they stand at an offset, so the packed row cannot sit beside them) they get their own slot,
-- left of the placed widgets with the normal gap, instead of the right edge where they would cover them.
local extras=table.clone(require("./recording"))
for _,node in require("../widgets/tray") do table.insert(extras,node) end
if c.islands then
    -- No border: the tray and the recording indicator share one small island, only while something shows in it.
    local kids=extras
    local shown=ui.resolve_for_check(ui.bind("bar.recording"))==true or ui.resolve_for_check(ui.bind("bar.reconnecting"))==true
        or (ui.resolve_for_check(ui.bind("bar.tray.visible"))==true and #(ui.resolve_for_check(ui.bind("bead.desktop.tray")) or {})>0)
    extras={}
    if shown then extras[1]=ui.Row(c.island{id="status-extras-island",height=c.t.pill_height,radius=c.t.pill_radius,gap=4,padding_left=8,padding_right=8,align="center",shrink=0,table.unpack(kids)}) end
end
if extras_right then
    extras.id,extras.position,extras.top,extras.height,extras.align,extras.gap,extras.right="status-extras","absolute",0,"100%","center",c.t.power_gap or 12,extras_right
    table.insert(placed,ui.Row(extras))
else regions.right.before=extras end
-- Placed widgets come after the title's band so they sit above it for input.
regions.centre.after=placed
-- B0211: drags draw nothing on the bar (no outline, no insertion slot); the ghost is the preview.
local host={edge="top",id="bar",shape="strip-h",grid={tiles=layout.tiles,mode="content",regions=regions},layout=flow,catalogue=widgets.catalogue,render=render,
    metadata=require("../widgets/editing").seed(canonical,width,ui.resolve_for_check(ui.bind("widgets.canvas.height")),"bar"),
    width="100%",height=c.header,radius=0,fill="surface",padding_left=15,padding_right=15,align="center",justify="space-between",direction="row",
}
if c.islands then
    -- No border: a transparent bar. The silk thread runs behind the islands (the first child), full strength on a
    -- clear desktop and 30 % while a window is on the active workspace; 220 ms, on the thread layer only.
    local busy=false
    for _,workspace in ui.resolve_for_check(ui.bind("bead.workspaces")) or {} do
        if workspace.active and workspace.occupied then busy=true end
    end
    busy=busy or ui.resolve_for_check(ui.bind("bar.panel.open"))==true
    local fade=ui.animate("bar.thread.fade",busy and .3 or 1,.22,"out-cubic")
    local segments={id="thread",position="absolute",left=-13,top=23,width=width,height=1,direction="row"}
    local count=48
    for i=1,count do
        local at=(i-.5)/count
        local colour=at<=.5 and ui.tint("primary","secondary",at*2) or ui.tint("secondary","tertiary",(at-.5)*2)
        segments[i]=ui.Box{id="s"..i,width=0,grow=1,height=1,fill=ui.alpha(colour,.6*fade*math.min(1,at/.09,(1-at)/.22))}
    end
    -- The 1 px dark under-line (a fuller orient thread over a dark under-stroke reads on bright skies), fading
    -- with the thread, 220 ms.
    segments[count+1]=ui.Image{id="under",position="absolute",left=0,top=1,width=width,height=1,path="bar/assets/thread-under.themed.svg",opacity=fade}
    host.fill,host.padding_left,host.padding_right,host.padding_top="transparent",13,13,2
    host[1]=ui.Row(segments)
    -- The apply glint: when the committed wallpaper changes (its luminance grid is republished then), one soft light
    -- (170 x 2.5, primary 40 % toward white) runs from the centre out along the thread to both edges, 2 s, behind
    -- the islands, translate and fill only. The key carries the grid's stamp, so each change starts a fresh run;
    -- an empty grid (no wallpaper yet) shows none.
    local luma=ui.resolve_for_check(ui.bind("bead.desktop.wallpaper.luma"))
    local grid=type(luma)=="table" and luma[1] or nil
    if grid and grid.cells then
        local stamp=0
        for i,cell in grid.cells do stamp+=cell*(i%7+1) end
        stamp=math.floor(stamp*1000)
        local glow=ui.tint("primary",{255,255,255,255},.4)
        local slices,slice_width=9,170/9
        for _,sign in {1,-1} do
            local run={id="glint"..sign,position="absolute",left=width/2-13-85,top=22,width=170,height=2.5,direction="row",
                translate_x=ui.animated("bar.glint.x"..sign.."."..stamp,sign*(width/2+85),2,"in-out-cubic",0)}
            for i=1,slices do
                run[i]=ui.Box{id="g"..i,width=slice_width,height=2.5,radius=1.25,
                    fill=ui.alpha(glow,ui.animated("bar.glint.a"..sign.."."..i.."."..stamp,0,2,"in-cubic",.85*math.sin(math.pi*(i-.5)/slices)))}
            end
            host[#host+1]=ui.Row(run)
        end
    end
    -- Clicks in the gaps between the islands reach the windows below (B0321).
    host.input="children"
end
return ui.Host(host)
