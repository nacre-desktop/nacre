local records = {}
local captured_rules = {}
local submap = ""
local function json(value)
    local kind = type(value)
    if kind == "nil" then return "null" end
    if kind == "boolean" or kind == "number" then return tostring(value) end
    if kind == "string" then
        return '"' .. value:gsub('[%z\1-\31\\"]', function(c)
            if c == '"' or c == '\\' then return '\\' .. c end
            return string.format('\\u%04x', string.byte(c))
        end) .. '"'
    end
    assert(kind == "table", "Unsupported binding value: " .. kind)
    local values = {}
    if #value > 0 then
        for _, item in ipairs(value) do values[#values+1] = json(item) end
        return '[' .. table.concat(values, ',') .. ']'
    end
    for key, item in pairs(value) do values[#values+1] = json(key) .. ':' .. json(item) end
    table.sort(values)
    return '{' .. table.concat(values, ',') .. '}'
end
local function dispatcher(path)
    return setmetatable({}, {
        __index = function(_, key) return dispatcher(path == "" and key or path .. "." .. key) end,
        __call = function(_, ...) return { method=path, args={...} } end,
    })
end
local stub = {
    dsp = dispatcher(""),
    bind = function(keys, action, flags)
        assert(type(keys) == "string", "Binding keys must be a string")
        if type(action) == "function" then action = {method="callback", args={}} end
        assert(type(action) == "table" and action.method, "Unsupported binding constructor")
        records[#records+1] = { keys=keys, action=action, flags=flags or {}, submap=(flags and flags.submap) or submap }
    end,
    define_submap = function(name, callback)
        local previous = submap; submap=name; callback(); submap=previous
    end,
}
stub.window_rule = function(rule) captured_rules[#captured_rules+1] = rule end
stub.layer_rule = function() end
stub.config = function() end
stub.on = function() end
local environment = {
    hl=stub, assert=assert, error=error, ipairs=ipairs, pairs=pairs,
    tonumber=tonumber, tostring=tostring, type=type, select=select,
    math=math, string=string, table=table,
}
local source, failure = loadfile(arg[1], "t", environment)
assert(source, failure)
source()
io.write(json(arg[2] == "rules" and captured_rules or records))
