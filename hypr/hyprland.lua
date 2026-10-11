-- Nacre owns the managed composition. Per-host/generated files load afterward.
-- These neutral fallbacks keep border consumers valid without a saved palette.
var_primary = "rgba(808080ff)"
var_on_primary = "rgba(f0f0f0ff)"

local modules = {
    "monitor",
    "cursor",
    "cursor-behavior",
    "keyboard",
    "autostart",
    "window",
    "decoration",
    "layout",
    "misc",
    "keybinding",
    "windowrule",
    "animation",
    "nacre",
    "brain",
}
for _, name in ipairs(modules) do
    require("conf." .. name)
end

local function load_private(name)
    local path = os.getenv("HOME") .. "/.config/nacre/" .. name .. ".lua"
    local handle = io.open(path, "r")
    if not handle then
        return
    end
    handle:close()

    local succeeded, reason = pcall(dofile, path)
    if not succeeded then
        hl.notification.create({
            text = "Nacre: " .. name .. " failed: " .. tostring(reason),
            timeout = 15000,
            icon = "error",
        })
    end
end

-- Precedence is deliberate: the user's host override is the final owner.
load_private("monitor")
-- Nacre committed wallpaper palette
load_private("palette")
-- Nacre desktop settings
load_private("desktop")
-- Nacre native desktop shortcuts
load_private("shortcuts")
load_private("profile")
load_private("host")
