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
    function commandFor(entry, directory = false) {
        const command = [...(entry?.command || [])];
        if (!command.length || command.some(argument => typeof argument !== "string"))
            return [];
        const launch = entry.runInTerminal ? ["kitty", "--", ...command] : command;
        return directory && entry.workingDirectory ? ["env", "-C", entry.workingDirectory, ...launch] : launch;
    }
    function nameForCommand(command) {
        const encoded = JSON.stringify(command);
        const exact = all.find(entry => JSON.stringify(commandFor(entry, true)) === encoded);
        if (exact)
            return exact.name;
        const executable = command?.[0] === "nacre-shell" && command?.[1] === "dolphin" ? "dolphin" : command?.[0] || "";
        return all.find(entry => entry.command?.[0]?.split("/").pop() === executable.split("/").pop())?.name || "";
    }
    function launch(entry) {
        const current = list.find(item => item === entry || item.id === entry?.id);
        if (!current)
            return false;
        const command = commandFor(current);
        if (!command.length)
            return false;
        AppLaunch.run(command, current.workingDirectory || undefined);
        return true;
    }
}
