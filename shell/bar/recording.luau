-- SPDX-License-Identifier: GPL-3.0-only
-- Recording indicator (B0199), QML's `nacreStatusRecording`: a red stop button with the elapsed time while a
-- recording runs; clicking it stops the recording. It only exists while `bar.recording` is true (the daemon's
-- `controls.recorder.recording`), so an idle bar pays nothing for it.
local ui=require("bead.ui")
local kit=require("bead.kit")
local t=require("../widgets/context").t
return {kit.Button{id="recording",visible=ui.bind("bar.recording"),direction="row",padding=0,fill="transparent",
    hover_fill="surfaceContainerHigh",height=t.status_height,radius=t.status_radius,gap=t.item_gap,shrink=0,
    padding_left=t.status_padding,padding_right=t.status_count_padding,align="center",justify="center",
    aria_label=ui.tr("bead.bar.stop-recording"),on_activate=ui.action("bar.recording.stop"),
    -- D1: radio_button_checked + elapsed in error (the pulse waits on kit ask 3).
    ui.Text{id="icon",text="radio_button_checked",family="Material Symbols Rounded",font_size=t.icon_font,width=t.icon_width,
        height=t.status_height,text_align="center",color="error"},
    ui.Text{id="elapsed",text=ui.bind("bar.recording.elapsed"),width=math.ceil(ui.text_width("00:00:00","IBM Plex Sans",t.clock_font)),
        height=t.status_height,font_size=t.clock_font,font_weight=500,color="error"}}}
