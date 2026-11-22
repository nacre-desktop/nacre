-- SPDX-License-Identifier: GPL-3.0-only
-- Desk popout: the desk list first (current marked), then Rename / Create as compact action rows, laid out like the tray menu.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local operation=get("collection.operation") or ""
local editing=operation~=""
local groups=get("bar.collections") or {}
local current=get("bar.collection.current") or ""
local height_limit=get("bar.collection.height") or 260
local W,PAD,GAP,ROW=300,12,2,30
local INNER=W-2*PAD
local selected=nil
for _,group in ipairs(groups) do if group.id==current then selected=group;break end end
local function label_of(group,i)
    if group.name then return group.name end
    return ui.tr("bead.bar.collection-unnamed",{index=group.index or i})
end
-- A transparent action/desk row: text left, optional trailing glyph, optional caption line under the text.
local function row(id,text,action,arg,opts)
    opts=opts or {}
    local h=opts.caption and 46 or ROW
    local inner=INNER-16
    local col={ui.Text{id="label",text=text,width=inner-(opts.glyph and 24 or 0),height=22,font_size=T.text.body*4/3,color="onSurface"}}
    if opts.caption then col[#col+1]=ui.Text{id="caption",text=opts.caption,width=inner,height=16,font_size=T.text.caption*4/3,color="onSurfaceVariant"} end
    local content=ui.Row{id="content",width=inner,height=h-8,align="center",gap=T.spacing.xs,ui.Column(col),
        opts.glyph and ui.Text{id="glyph",text=opts.glyph,family=T.fonts.icon,width=20,height=20,font_size=T.icons.medium*4/3,color="primary"} or nil}
    return kit.Button{id=id,width="fill",height=h,padding=4,padding_left=8,padding_right=8,fill=opts.selected and "surfaceContainerHigh" or "transparent",hover_fill=ui.alpha("onSurface",.06),
        aria_label=opts.aria or text,on_activate=ui.action(action,arg),content}
end
local body={}
-- Heights are tracked by hand: a built node does not expose its `height`.
local heights={}
local function add(node,h) body[#body+1]=node;heights[#heights+1]=h end
if editing then
    add(kit.TextInput{id="group-name",text=ui.bind("collection.draft"),placeholder=ui.tr("bead.bar.collection-desk-name"),
        aria_label=ui.tr("bead.bar.collection-desk-name"),on_activate=ui.action("collection.commit")},40)
    add(ui.Row{id="editor-actions",width="fill",height=40,shrink=0,gap=T.spacing.xs,align="center",justify="end",
        kit.Button{id="save-name",width=88,shrink=0,text=ui.tr("bead.bar.collection-save"),on_activate=ui.action("collection.commit")},
        kit.Button{id="cancel-name",width=88,shrink=0,text=ui.tr("bead.bar.collection-cancel"),on_activate=ui.action("collection.cancel")}},40)
else
    local desks={id="desks",width="fill",gap=GAP,scroll=ui.bind("collection.scroll")}
    for i,group in ipairs(groups) do
        desks[#desks+1]=row("group-"..tostring(group.id),label_of(group,i),"collection.switch",group.id,
            {selected=group.id==current,glyph=group.id==current and "check" or nil})
    end
    local desks_height=math.min(height_limit,#groups*ROW+math.max(#groups-1,0)*GAP)
    desks.height=desks_height
    add(ui.Scroll(desks),desks_height)
    add(ui.Box{id="divider",width="fill",height=1,shrink=0,fill="outlineVariant"},1)
    if selected then
        -- The unnamed desk's hint lives on the row that fixes it.
        add(row("rename-"..tostring(current),ui.tr("bead.bar.collection-rename"),"collection.begin-rename",current,
            {caption=not selected.name and ui.tr("bead.bar.collection-name-required") or nil}),selected.name and ROW or 46)
    end
    add(row("create",ui.tr("bead.bar.collection-create"),"collection.begin-create"),ROW)
    if selected and selected.name then
        add(ui.Box{id="divider-wallpaper",width="fill",height=1,shrink=0,fill="outlineVariant"},1)
        for _,action in ipairs({
            {"wallpaper","bead.bar.collection-set-wallpaper","collection.wallpaper.set"},
            {"accent","bead.bar.collection-wallpaper-accent","collection.accent.use"},
            {"base-accent","bead.bar.collection-base-accent","collection.accent.clear"},
        }) do add(row("host-"..action[1],ui.tr(action[2]),action[3],current),ROW) end
        if get("bar.collection.wallpapers.enabled")~=true then
            add(ui.Text{id="wallpaper-setting-note",text=ui.tr("bead.bar.collection-wallpapers-disabled"),width=INNER,height=32,font_size=T.text.caption*4/3,color="onSurfaceVariant"},32)
        end
    end
end
local panel_height=2*PAD+(#heights-1)*GAP
for _,h in ipairs(heights) do panel_height+=h end
return kit.Panel{id="collection-switcher",edge=get("view.edge"),
    open=ui.bind("panel.open"),fade=false,reveal=true,
    width=W,height=panel_height,padding=PAD,gap=GAP,
    radius=T.radii.medium,fill="surface",
    table.unpack(body),
}
