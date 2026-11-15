-- SPDX-License-Identifier: GPL-3.0-only
-- Store defaults for the Nacre AI window (N0248). `nacre-shell/src/ai_app.rs` projects every
-- data value from the N0319 topics; the rail page and the chosen provider and project are view
-- state. An empty provider or project means "the first one listed".
return {stores={
    ["window.width"]=1120,["window.height"]=760,["window.min_width"]=760,["window.min_height"]=520,
    ["ai.page"]="chats",["ai.provider"]="",["ai.project"]="",["ai.scroll"]=0,
    ["ai.win_w"]=1120,["ai.connected"]=false,["ai.loaded"]=false,["ai.message"]="",
    ["ai.can.start"]=false,["ai.can.resume"]=false,["ai.can.refresh"]=false,
    ["ai.providers"]={},["ai.projects"]={},["ai.accounts"]={},["ai.usage"]={},["ai.chats"]={},
},actions={
    ["ai.page"]=function(s,id) s["ai.page"]=id s["ai.scroll"]=0 end,
    ["ai.provider"]=function(s,id) s["ai.provider"]=id end,
    ["ai.project"]=function(s,id) s["ai.project"]=id end,
    -- The AI app adapter authorizes and handles this host action (start, resume, refresh, brain, settings).
    ["ai.do"]=function(_) end,
},computed={}}
