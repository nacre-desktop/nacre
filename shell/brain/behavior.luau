-- SPDX-License-Identifier: GPL-3.0-only
-- Store defaults for the Brain window (N0217). `nacre-shell/src/brain.rs` projects every
-- `brain.*` data value from the on-demand worker; screen, query, open note and history are view
-- state. Worlds take palette roles by world order (`WORLD_ROLES` in main.luau), never literals.
return {stores={
    ["window.width"]=1280,["window.height"]=800,["window.min_width"]=760,["window.min_height"]=540,
    ["brain.screen"]="overview",["brain.query"]="",["brain.world"]="",["brain.note"]="",["brain.note_path"]="",
    ["brain.open"]="",["brain.history"]={},["brain.scroll"]=0,["brain.rail.scroll"]=0,["brain.ask.text"]="",
    ["brain.status"]="connecting",["brain.error"]="",["brain.counts"]={},["brain.worlds"]={},["brain.recent"]={},
    ["brain.loops"]={},["brain.results"]={},["brain.result_count"]=0,["brain.did_you_mean"]="",["brain.page"]={},
    ["brain.ask.enabled"]=false,["brain.ask.busy"]=false,["brain.ask.answer"]="",["brain.ask.error"]="",["brain.ask.citations"]={},
    ["brain.graph_available"]=false,["brain.win_w"]=1280,
},actions={
    -- The Brain adapter authorizes and handles this host action (screen, open, back, world, ask, action...).
    ["brain.do"]=function(_) end,
},computed={}}
