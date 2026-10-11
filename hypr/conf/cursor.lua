-- UWSM owns the cursor preference; apply its values once when Hyprland starts.
local function apply_session_cursor()
    local theme = os.getenv("XCURSOR_THEME")
    local size = math.tointeger(tonumber(os.getenv("XCURSOR_SIZE") or ""))
    if not theme or theme == "" or not size or size < 1 or size > 2147483647 then
        return
    end

    local argument = "'" .. theme:gsub("'", "'\\''") .. "'"
    hl.exec_cmd("hyprctl setcursor " .. argument .. " " .. tostring(size))
end

hl.on("hyprland.start", apply_session_cursor)
