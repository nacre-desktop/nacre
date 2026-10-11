-- Systemd owns the enabled desktop/idle/Brain services; this owns session helpers.
local function shell_argument(value)
    return "'" .. value:gsub("'", "'\\''") .. "'"
end

hl.on("hyprland.start", function()
    local scripts = os.getenv("HOME") .. "/.config/hypr/scripts/"
    local tasks = {
        { scope = true, args = { "/usr/lib/pam_kwallet_init" } },
        { args = { "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1" } },
        { args = { "wl-paste", "--watch", "cliphist", "store" } },
        { args = { scripts .. "startup-apps.sh" } },
        { args = { os.getenv("HOME") .. "/.local/bin/nacre-welcome", "--login" } },
    }
    for _, task in ipairs(tasks) do
        local command = task.scope and "uwsm app -t scope --" or "uwsm app --"
        for _, argument in ipairs(task.args) do
            command = command .. " " .. shell_argument(argument)
        end
        hl.exec_cmd(command)
    end
end)
