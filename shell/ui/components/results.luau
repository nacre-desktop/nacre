-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local VirtualList=require("bead.widgets.kit.virtual-list")
local M=require("./metrics")
local Icon=require("./item-icon")
local Lines=require("./text-lines")
local Row=ui.component("ResultRow",{item={id="string",name="string",icon="string",selected="boolean",description="string",favorite="boolean",favoriteId="string"}},function(p)
    local item=p.item
    local toggle=item.toggle==true
    local value_group=type(item.choices)=="table" and #item.choices>0
    local width=M.content_width(ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("launcher.quick_allowed")))-12-(toggle and 110 or value_group and 72 or 42)-M.result_icon-11
    local activate=ui.action("select_app",item.id)
    return ui.Box{id="result-wrap",width="fill",height=M.result_row_height,
      kit.Button{id="result",width="fill",height="fill",radius=10,padding_left=12,padding_right=toggle and 82 or 42,padding_top=8,padding_bottom=8,fill="transparent",selected=item.selected,border_color="transparent",
        selected_fill=ui.tint("surface","onSurface",.09),hover_fill=ui.tint("surface","onSurface",.04),
        on_activate=activate,aria_label=item.name..(item.status and (". "..item.status) or ""),
        ui.Box{position="absolute",left=0,top=9,width=3,height=37,radius=1.5,fill="primary",visible=item.selected},
        ui.Row{gap=11,align="center",grow=1,
            type(item.emoji)=="string" and item.emoji~="" and ui.Text{text=item.emoji,font_size=24,width=M.result_icon,height=M.result_icon} or Icon(item,M.result_icon),
            ui.Column{width=width,gap=3,justify="center",
                Lines{id="name",text=item.name,width=width,font_size=M.result_title_font,line_height=M.result_title_height,max_lines=1,color=item.disabled and "onSurfaceVariant" or "onSurface"},
                Lines{id="description",text=value_group and (item.status and item.status~="" and item.status or item.description) or item.description,width=width,font_size=M.result_description_font,line_height=M.result_description_height,max_lines=2,color="onSurfaceVariant"},
            },
        },
        value_group and ui.Text{id="chevron",position="absolute",right=10,top=17,width=20,height=20,text="chevron_right",family="Material Symbols Rounded",font_size=18,color="onSurfaceVariant"} or nil,
      },
      ui.Button{id="favorite",position="absolute",right=value_group and 38 or 12,top=16,width=23,height=23,radius=7,padding=0,
        visible=not toggle and (item.favorite or item.selected or M.hovered(item.id)),
        aria_label=item.favorite and ui.tr("bead.launcher.favorite-remove") or ui.tr("bead.launcher.favorite-add"),
        fill="transparent",justify="center",align="center",
        on_activate=ui.action("toggle_favorite",item.id),
        ui.Image{path=item.favorite and "ui/components/assets/favorite-1.themed.svg" or "ui/components/assets/favorite-0.themed.svg",width=11*4/3,height=11*4/3},
      },
      -- Optional children stay last: a nil in the middle of the child list ends it.
      toggle and ui.Text{id="toggle-state",position="absolute",right=67,top=19,width=34,height=18,text=item.status,font_size=11.333,text_align="right",color="onSurfaceVariant"} or nil,
      toggle and kit.Switch{id="toggle-switch",position="absolute",right=10,top=14,width=44,height=28,checked=item.checked,aria_label=item.name..". "..item.status,on_activate=not item.disabled and ui.action("select_app",item.id) or nil} or nil,
    }
end)
return ui.component("Results",{},function(_)
    local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width"))
    local width=M.content_width(viewport,ui.resolve_for_check(ui.bind("launcher.quick_allowed")))
    return VirtualList{id="results",scroll=ui.bind("launcher.scroll.y"),width=width,height=400,grow=1,
        gap=3,row_height=M.result_row_height,total_count=ui.bind("launcher.total_count"),model=ui.bind("launcher.apps"),key="id",render=function(item)return Row{item=item}end}
end)
