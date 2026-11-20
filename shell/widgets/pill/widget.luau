-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local c=require("../context")
local t=c.t
return {id="pill",title="Pill",icon="pill",category="containers",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},forms={bar={default_style="pill",styles={pill={title="Pill",min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
    -- D1: items tile with no seam (each carries half the 14 px gap); 12 px from the pill ends to the first glyph box.
    local gap=t.pill_gap
    local end_pad=t.status_end_padding
    -- The pill's ends carry no padding: the first and last item's hover (a 17 px stadium, the pill's own rounding)
    -- reaches the pill's outer edge, and that item takes the end padding itself so its glyph keeps its place.
    local items=props.instance.children or {}
    local function reach(node,left,right)
        local p=node.props
        if p and p.hover_fill then
            if left then p.padding_left=(p.padding_left or p.padding or 0)+end_pad end
            if right then p.padding_right=(p.padding_right or p.padding or 0)+end_pad end
            return true
        end
        for _,child in node.children or {} do if reach(child,left,right) then return true end end
        return false
    end
    -- Render every item first: an item can draw nothing (no updates, a hidden battery), so the ends are the first
    -- and last item that shows. The host then takes the finished nodes.
    local nodes,shown={},{}
    for _,item in items do
        local package=props.catalogue[item.widget]
        local node=package and props.render(item,package.variants.bar)
        nodes[item]=node or false
        if node and not (node.props and node.props.visible==false) then table.insert(shown,item) end
    end
    for _,item in shown do reach(nodes[item],item==shown[1],item==shown[#shown]) end
    local function render(item) return nodes[item] or nil end
    return kit.Pill(c.island{id="system-pill",height=t.pill_height,gap=gap,shrink=0,padding_left=0,padding_right=0,align="center",fill=c.base,radius=t.pill_radius,clip_children=true,
        ui.Host{id="pill-host",shape="strip-h",flatten=true,grid={tiles=60,mode="content",regions={left={inline=true},centre={props={id="status",height=t.pill_height,gap=gap,shrink=0,align="center"}},right={inline=true}}},layout=items,catalogue=props.catalogue,render=render},
    })
end}}}},panel={styles={control={view="control"}}}}
