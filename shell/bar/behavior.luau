-- SPDX-License-Identifier: GPL-3.0-only
return {stores={["bead.desktop.wallpaper.luma"]={},["widgets.undo"]={},["widgets.error"]="",["widgets.message"]="",["widgets.preview"]={},["widgets.draft"]={},["widgets.geometry"]={},["widgets.editing"]=false,["widgets.selected"]="",["widgets.canvas.width"]=0,["widgets.canvas.height"]=0,["widgets.drag.active"]=false,["widgets.drag.source"]={},["widgets.drag.phase"]="",["widgets.drag.epoch"]=0,
    ["widgets.app.open"]=false,["widgets.drag.layout"]={},["widgets.drag.base"]={},["widgets.drag.original"]={},["widgets.drag.destination"]="",
    ["widgets.drag.x"]=0,["widgets.drag.y"]=0,["widgets.drag.start_x"]=0,["widgets.drag.start_y"]=0,
    ["widgets.drag.width"]=1,["widgets.drag.height"]=1,["widgets.drag.grab_x"]=0,["widgets.drag.grab_y"]=0,
    ["widgets.drag.snap_x"]=0,["widgets.drag.snap_y"]=0,
    ["widgets.drag.source_origin_x"]=0,["widgets.drag.source_origin_y"]=0,["widgets.drag.source_origin_valid"]=false,
    ["widgets.drag.target_origin_x"]=0,["widgets.drag.target_origin_y"]=0,
    ["widgets.drag.app_origin_x"]=0,["widgets.drag.app_origin_y"]=0,["widgets.drag.app_origin_valid"]=false,
    ["widgets.drag.app_width"]=0,["widgets.drag.app_height"]=0,["widgets.drag.target_output_id"]="",
    ["widgets.group"]="",["widgets.layout"]={},["widgets.rev"]=0,["widgets.outputs"]={},["widgets.screen_width"]=0,["widgets.screen_height"]=0,["widgets.screen_origin_x"]=0,["widgets.screen_origin_y"]=0,["widgets.screen_id"]="",["bar.height"]=50,["bar.notifications.count"]=0,["bar.launcher"]=false,["bar.popouts"]=false,["bar.reconnecting"]=false,["bar.recording"]=false,["bar.recording.elapsed"]="",["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.window.focus.enabled"]=true,["bar.workspace.origins"]={},["bar.collections"]={},["bar.collection.current"]="",["bar.collection.connection"]="",["bar.collection.enabled"]=false,["bar.collection.wallpapers.enabled"]=false,["bar.collection.height"]=260,["collection.draft"]="",["collection.operation"]="",["collection.target"]="",["collection.scroll"]=0,["bar.dashboard.enabled"]=true,
    ["bar.tray.visible"]=false,["bar.workspace.count"]=6,["bar.workspace.icons"]={},["bar.workspace.hints"]=true,["bar.workspace.style"]="pearls",["bar.workspace.overflow"]=true,["bar.clock.twelve"]=false,["bar.updates.count"]=0,["bar.updates.enabled"]=true,["bar.modules"]={},["bar.dnd"]=false,["bar.stack.style"]="pearls",["bar.status.badges"]=true,
    ["bead.edge.dwell"]=0,["bead.edge.fade"]=0,
    ["bead.overview.open"]=false,["bead.overview.title"]="",["bead.overview.has_window"]=false,
    ["bead.title.app_id"]="",["bead.title.rev"]=0,["bead.title.app_glyph"]="desktop_windows",["bead.title.app_icon"]="",
    ["bead.desktop.network.vpn"]=false,["bead.desktop.bluetooth.value"]="",
    ["status.hovered"]="",["status.peek.visible"]=false,["status.peek.kind"]="",
    ["status.panel.kind"]=""},actions={["launcher.open"]=function(_) end,["bar.recording.stop"]=function(_) end,["widgets.snapshot"]=function(_) end,["widgets.edit"]=function(_) end,["widgets.drag"]=function(_) end,
    ["collection.switcher.open"]=function(_) end,["collection.switch"]=function(_) end,["collection.begin-create"]=function(_) end,["collection.begin-rename"]=function(_) end,["collection.commit"]=function(_) end,["collection.cancel"]=function(_) end,["collection.wallpaper.set"]=function(_) end,["collection.accent.use"]=function(_) end,["collection.accent.clear"]=function(_) end},computed={
    -- desktop.clockFormat: the daemon clock is "HH:MM"; 12-hour shows "h:MM AM/PM".
    ["bar.clock.text"]=function(s)
        local c=s["bead.desktop.clock"]
        local h,m=string.match(c,"^(%d+):(%d%d)$")
        if not s["bar.clock.twelve"] or not h then return c end
        local n=tonumber(h)
        return string.format("%d:%s %s",(n+11)%12+1,m,n<12 and "AM" or "PM")
    end,
    ["bar.updates.text"]=function(s) return tostring(s["bar.updates.count"]) end,
    ["bar.clock.width"]=function(s) return s["bar.clock.twelve"] and 66 or 44 end,
    ["bar.title.cue.visible"]=function(s) return s["bead.edge.fade"]>0 end,
    ["bar.title.cue.fade"]=function(s) return s["bead.edge.fade"] end,
    ["bar.title.cue.halo"]=function(s) return .025*s["bead.edge.dwell"]*s["bead.edge.fade"] end,
    ["bar.title.cue.body"]=function(s) return (.03+.09*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
    ["bar.title.cue.border"]=function(s) return (.06+.18*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
    ["bar.notifications.text"]=function(s) return tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.tail"]=function(s) return 7+7.5*#tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.detail.width"]=function(s) return 1+8*#s["bar.notifications.text"] end,
    ["bar.battery.detail.width"]=function(s) return 1+8*#s["bead.desktop.battery.value"] end,
    -- QML NacreActiveTitle: with no focused window the title reads "Desktop".
    ["bar.title"]=function(s)
        local t=s["bead.overview.open"] and s["bead.overview.title"] or ""
        if t=="" and s["bead.overview.open"] and s["bead.overview.has_window"] then t=tr("bead.bar.active-window") end
        if t=="" then t=s["bead.focused_title"] end
        return t~="" and t or tr("bead.bar.desktop-title")
    end,
}}
