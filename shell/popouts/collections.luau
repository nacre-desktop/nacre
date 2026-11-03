-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local operation=ui.resolve_for_check(ui.bind("collection.operation")) or ""
local editing=operation~="" and {value="collection.draft",label=ui.tr("nacre.collection-switcher.desk-name")} or nil
local groups=ui.resolve_for_check(ui.bind("bar.collections")) or {}
local current=ui.resolve_for_check(ui.bind("bar.collection.current")) or ""
local height_limit=ui.resolve_for_check(ui.bind("bar.collection.height")) or 260
local selected=nil
for _,group in ipairs(groups) do if group.id==current then selected=group;break end end
local actions={}
local note=nil
if selected and selected.name then
    if ui.resolve_for_check(ui.bind("bar.collection.wallpapers.enabled"))~=true then
        note=ui.Text{id="wallpaper-setting-note",text=ui.tr("nacre.collection-switcher.enable-wallpapers"),width=272,height=38,font_size=kit.tokens.text.body,color="onSurfaceVariant"}
    end
    actions={
        {id="wallpaper",label=ui.tr("nacre.collection-switcher.set-wallpaper"),action="collection.wallpaper.set",arg=ui.bind("bar.collection.current")},
        {id="accent",label=ui.tr("nacre.collection-switcher.use-wallpaper-accent"),action="collection.accent.use",arg=ui.bind("bar.collection.current")},
        {id="base-accent",label=ui.tr("nacre.collection-switcher.use-base-accent"),action="collection.accent.clear",arg=ui.bind("bar.collection.current")},
    }
else
    note=ui.Text{id="name-required",text=ui.tr("nacre.collection-switcher.name-to-save-wallpaper"),width=272,height=38,font_size=kit.tokens.text.body,color="onSurfaceVariant"}
end
local edit_height=editing and 80 or 36
local rows_height=#groups*76+edit_height+#actions*42+12
local menu_height=math.min(height_limit,rows_height)
local panel_height=menu_height+(note and 58 or 28)
return kit.Panel{id="collection-switcher",edge=ui.resolve_for_check(ui.bind("view.edge")),
    open=ui.bind("panel.open"),duration=.23,transition="ease-in-out",fade=false,reveal=true,
    width=300,height=panel_height,
    padding=14,radius=kit.tokens.radii.medium,fill="surface",
    note,
    kit.CollectionSwitcherMenu{
        id="collection-switcher-menu",width=272,height=menu_height,scroll=ui.bind("collection.scroll"),
        groups=ui.bind("bar.collections"),current=ui.bind("bar.collection.current"),
        on_switch="collection.switch",on_create="collection.begin-create",on_rename="collection.begin-rename",
        on_commit="collection.commit",on_cancel="collection.cancel",editing=editing,
        create_label=ui.tr("nacre.collection-switcher.create"),rename_label=ui.tr("nacre.collection-switcher.rename"),
        save_label=ui.tr("nacre.collection-switcher.save"),cancel_label=ui.tr("nacre.collection-switcher.cancel"),
        actions=actions,
    },
}
