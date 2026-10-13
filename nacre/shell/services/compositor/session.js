function selectBackend(environment) {
    const explicit = String(environment.NACRE_COMPOSITOR || "").toLowerCase();
    if (explicit)
        return explicit === "hyprland" || explicit === "niri" ? explicit : "unsupported";
    const desktops = String(environment.XDG_CURRENT_DESKTOP || "").toLowerCase().split(":");
    if (desktops.includes("niri"))
        return "niri";
    if (environment.HYPRLAND_INSTANCE_SIGNATURE)
        return "hyprland";
    return environment.NIRI_SOCKET ? "niri" : "hyprland";
}
