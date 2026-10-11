pragma Singleton
import Quickshell
import "app-search.js" as Search

Singleton {
    id: root
    readonly property var all: {
        const entries = new Map();
        for (const entry of DesktopEntries.applications.values)
            if (entry && !entry.noDisplay && entry.id && !entries.has(entry.id))
                entries.set(entry.id, entry);
        return [...entries.values()].sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id));
    }
    readonly property var list: all.filter(entry => !LauncherPreferences.hidden.includes(entry.id))
    readonly property var preppedApps: list.map(entry => Search.prepare(entry))
    function fuzzyQuery(search) {
        return Search.query(preppedApps, search);
    }
    function launch(entry) {
        const current = list.find(item => item === entry || item.id === entry?.id);
        if (!current)
            return false;
        const command = [...(current.command || [])];
        if (!command.length || command.some(argument => typeof argument !== "string"))
            return false;
        AppLaunch.run(current.runInTerminal ? ["kitty", "--", ...command] : command, current.workingDirectory || undefined);
        return true;
    }
}
