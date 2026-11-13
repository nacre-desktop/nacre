-- SPDX-License-Identifier: GPL-3.0-only
-- Control-center tile registry (B0049). A tile is data: {id, kind, icon, label, source, view|page, default}.
-- The overview groups the saved order by form (N0238, design review D4): sliders are full rows, toggles and
-- links are 2 x 2 tiles, and buttons, the appearance segment and `chip=true` toggles are chips; the edit page
-- orders all of them in one list. `source` names a data source the host publishes in the `cc.src` store (value, state and which
-- verbs it accepts); `view` opens another panel; `page` opens an in-panel detail page. A tile whose source
-- or view is absent is unavailable and only listed in the catalog with its `ask` (the missing contract), so
-- it appears the moment the host publishes it. Add or reorder entries here: no Rust change is needed for a
-- tile over an existing source. A new source is one entry in `control_center.rs`.
local T=require("bead.kit").tokens
local layout={
    width=T.spacing.huge*13+T.spacing.sm,
    -- a slider row and the 2 x 2 tile grid; heights are multiples of the shared scale
    sizes={full={per_row=1,height=56},half={per_row=2,height=64}},
}
local tiles={
    {id="volume",kind="slider",icon="volume_up",label="bead.panel.control-volume",source="audio.output",dock="audio",hide_unsupported=true,default=true},
    {id="mic-volume",kind="slider",icon="mic",label="bead.panel.control-mic-volume",source="audio.input",dock="audio",hide_unsupported=true,ask="audio.set_volume",need="set",default=false},
    {id="brightness",kind="slider",icon="brightness_6",label="bead.panel.control-brightness",source="brightness.level",dock="battery",hide_unsupported=true,ask="brightness.set",need="set",default=true},
    {id="keyboard",kind="slider",icon="keyboard",label="bead.panel.control-keyboard",source="keyboard.backlight",dock="battery",hide_unsupported=true,ask="keyboard_light.set",need="set"},
    {id="network",kind="toggle",icon="wifi",label="bead.panel.control-network",source="network.wifi",view="network",dock="network",default=true},
    {id="bluetooth",kind="toggle",icon="bluetooth",label="bead.panel.control-bluetooth",source="bluetooth.power",view="bluetooth",dock="bluetooth",default=true},
    {id="airplane",kind="toggle",icon="flight",label="bead.panel.control-airplane",source="airplane.mode",ask="airplane.set",need="toggle"},
    {id="vpn",kind="toggle",icon="vpn_key",label="bead.panel.control-vpn",source="vpn.state",dock="vpn",navigate=true,hide_unsupported=true,ask="vpn"},
    {id="battery",kind="toggle",icon="balance",label="bead.panel.control-battery",source="power.profile",view="battery",dock="battery",ask="power.set_profile",default=true},
    {id="awake",kind="toggle",icon="coffee",label="bead.panel.control-awake",source="stay-awake.state",ask="stay_awake.set",need="toggle",default=true},
    {id="dnd",kind="toggle",icon="do_not_disturb_on",label="bead.panel.control-dnd",source="notifications.dnd",page="notifications",default=true},
    {id="night",kind="toggle",chip=true,icon="nightlight",label="bead.panel.control-night",source="night-light.state",hide_unsupported=true,ask="night_light.set",default=true},
    {id="appearance",kind="segmented",icon="palette",label="bead.panel.control-appearance",source="appearance.mode",ask="appearance.set_mode",need="toggle",default=true},
    {id="screenshot",kind="button",icon="screenshot",label="bead.panel.control-screenshot",source="screenshot.capture",hide_unsupported=true,ask="screenshot.capture",need="press"},
    {id="record",kind="toggle",chip=true,icon="videocam",label="bead.panel.control-record",source="recorder.state",hide_unsupported=true,ask="recorder.start",need="toggle"},
    {id="picker",kind="button",icon="colorize",label="bead.panel.control-picker",source="color-picker.pick",hide_unsupported=true,ask="color_picker.pick",need="press"},
    {id="clipboard",kind="button",icon="content_paste",label="bead.panel.control-clipboard",open="clipboard"},
    {id="updates",kind="button",icon="system_update",label="bead.panel.control-updates",view="dashboard-system"},
    {id="displays",kind="button",icon="desktop_windows",label="bead.panel.control-displays",source="displays.layout",ask="displays topic"},
    {id="wallpaper",kind="button",icon="wallpaper",label="bead.panel.control-wallpaper",open="settings:appearance"},
    {id="next-wallpaper",kind="button",icon="skip_next",label="bead.panel.control-next-wallpaper",source="wallpaper.next",ask="wallpaper.next",need="press"},
    {id="notifications",kind="button",icon="notifications",label="bead.panel.control-notifications",page="notifications"},
}
-- every tile is in the default layout except the ones the sound page already covers (mic volume)
for _,tile in ipairs(tiles) do tile.default=tile.default~=false end
local by_id={}
for _,t in ipairs(tiles) do by_id[t.id]=t end
return {layout=layout,tiles=tiles,by_id=by_id}
