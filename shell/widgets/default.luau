-- SPDX-License-Identifier: GPL-3.0-only
-- The content-packed preset retains the QML bar's fractional-pixel geometry.
local function instance(iid,widget,host,region,start,settings)
    return {iid=iid,widget=widget,host=host,slot={region=region,start=start,span=1},settings=settings or {}}
end
return function(width, selected_modules, source_document)
    local tiles=math.max(6,math.floor((width or 1920)/32+.5))
    local pill=instance("status-pill","pill","top_bar","right",0)
    local children={
        instance("updates","updates","status-pill","left",0),
        instance("sound","sound","status-pill","centre",0),
        instance("wifi","wifi","status-pill","centre",1),
        instance("bluetooth","bluetooth","status-pill","centre",2),
        instance("battery","battery","status-pill","centre",3),
        instance("notifications","notifications","status-pill","centre",4),
        instance("clock","clock","status-pill","right",0),
        instance("power","power","status-pill","right",1),
    }
    pill.children=children
    local defaults={
        instance("launcher","launcher","top_bar","left",0,{icon="nacre",custom_path="",action="launcher",route=""}),
        instance("workspaces","workspaces","top_bar","left",1),
        instance("title","title","top_bar","centre",0),
    }
    local default_instances=table.clone(defaults)
    table.insert(default_instances,pill)
    local document=source_document or {version=1,preset="Nacre default",hosts={top_bar={tiles=tiles,mode="content",regions={left={start=0,span=tiles},centre={start=0,span=tiles},right={start=0,span=tiles}},instances=default_instances}}}
    if type(selected_modules)~="table" or #selected_modules==0 then
        return document
    end
    local selected={}
    for _,item in ipairs(selected_modules) do
        if type(item)=="table" and type(item.id)=="string" then selected[item.id]=true end
    end
    local known={}
    for _,item in ipairs(defaults) do known[item.iid]=true end
    known[pill.iid]=true
    for _,item in ipairs(children) do known[item.iid]=true end
    local copy=table.clone(document)
    copy.hosts=table.clone(document.hosts)
    local host=table.clone(document.hosts.top_bar)
    copy.hosts.top_bar=host
    host.instances={}
    for _,item in ipairs(document.hosts.top_bar.instances) do
        if item.iid==pill.iid then
            local cloned=table.clone(item)
            cloned.children={}
            for _,child in ipairs(item.children or {}) do
                if not known[child.iid] or selected[child.iid] then table.insert(cloned.children,child) end
            end
            if #cloned.children>0 then table.insert(host.instances,cloned) end
        elseif not known[item.iid] or selected[item.iid] then
            table.insert(host.instances,item)
        end
    end
    return copy
end
