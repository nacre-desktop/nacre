-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local operation=ui.resolve_for_check(ui.bind("collection.operation")) or ""
local editing=operation~="" and {value="collection.draft",label=ui.tr("bead.bar.collection-desk-name")} or nil
local groups=ui.resolve_for_check(ui.bind("bar.collections")) or {}
local current=ui.resolve_for_check(ui.bind("bar.collection.current")) or ""
local target=ui.resolve_for_check(ui.bind("collection.target")) or current
local menu_groups=groups
if editing then
    menu_groups={}
    for _,group in ipairs(groups) do if group.id==target then menu_groups[1]=group;break end end
end
local height_limit=ui.resolve_for_check(ui.bind("bar.collection.height")) or 260
local selected=nil
for _,group in ipairs(groups) do if group.id==current then selected=group;break end end
local actions={}
local note=nil
if editing then
    note=nil
elseif selected and selected.name then
    if ui.resolve_for_check(ui.bind("bar.collection.wallpapers.enabled"))~=true then
        note=ui.Text{id="wallpaper-setting-note",text=ui.tr("bead.bar.collection-wallpapers-disabled"),width=272,height=38,font_size=kit.tokens.text.body,color="onSurfaceVariant"}
    end
    actions={
        {id="wallpaper",label=ui.tr("bead.bar.collection-set-wallpaper"),action="collection.wallpaper.set",arg=current},
        {id="accent",label=ui.tr("bead.bar.collection-wallpaper-accent"),action="collection.accent.use",arg=current},
        {id="base-accent",label=ui.tr("bead.bar.collection-base-accent"),action="collection.accent.clear",arg=current},
    }
else
    note=ui.Text{id="name-required",text=ui.tr("bead.bar.collection-name-required"),width=272,height=38,font_size=kit.tokens.text.body,color="onSurfaceVariant"}
end
local edit_height=editing and 80 or 36
local rows_height=#menu_groups*(editing and 40 or 76)+edit_height+#actions*42+12
local menu_height=math.min(height_limit,rows_height)
local panel_height=menu_height+(note and 58 or 28)
local body={}
if note then body[#body+1]=note end
body[#body+1]=kit.CollectionSwitcherMenu{
    id="collection-switcher-menu",width=272,height=menu_height,scroll=ui.bind("collection.scroll"),
    groups=menu_groups,current=current,
    on_switch=not editing and "collection.switch" or nil,on_create=not editing and "collection.begin-create" or nil,on_rename=not editing and "collection.begin-rename" or nil,
    on_commit="collection.commit",on_cancel="collection.cancel",editing=editing,
    create_label=ui.tr("bead.bar.collection-create"),rename_label=ui.tr("bead.bar.collection-rename"),
    save_label=ui.tr("bead.bar.collection-save"),cancel_label=ui.tr("bead.bar.collection-cancel"),
    actions=not editing and actions or {},
}
return kit.Panel{id="collection-switcher",edge=ui.resolve_for_check(ui.bind("view.edge")),
    open=ui.bind("panel.open"),duration=.23,transition="ease-in-out",fade=false,reveal=true,
    width=300,height=panel_height,
    padding=14,radius=kit.tokens.radii.medium,fill="surface",
    table.unpack(body),
}
