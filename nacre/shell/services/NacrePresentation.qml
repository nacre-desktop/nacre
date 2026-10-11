pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils
import "colour-data.js" as Colours

Singleton {
    id: root
    property var state: ({
            pending: {},
            active: {}
        })
    readonly property var pending: state.pending
    readonly property var active: state.active
    readonly property bool available: !!pending.colours
    property string error: ""
    property int revision: 0
    function canonicalPoster(value) {
        if (typeof value !== "string" || !value || value.includes("\u0000"))
            return "";
        if (value.startsWith("file://")) {
            let path = value.slice(7);
            if (path.startsWith("localhost/"))
                path = path.slice(9);
            if (!path.startsWith("/"))
                return "";
            try {
                return decodeURIComponent(path);
            } catch (failure) {
                return "";
            }
        }
        return value.startsWith("/") ? value : "";
    }
    function validate(data) {
        if (!data || Array.isArray(data) || !["dark", "light"].includes(data.mode) || !canonicalPoster(data.poster) || !data.colours || Array.isArray(data.colours))
            return false;
        if (Object.keys(Colours.fallback).some(key => typeof data.colours[key] !== "string" || !/^#?[0-9a-fA-F]{6}$/.test(data.colours[key])))
            return false;
        if (data.paletteOptions !== undefined && !Array.isArray(data.paletteOptions))
            return false;
        if (data.selectedAccent !== undefined && data.selectedAccent !== null && typeof data.selectedAccent !== "string")
            return false;
        if (data.changedAtMs !== undefined && (!Number.isFinite(data.changedAtMs) || data.changedAtMs < 0))
            return false;
        if (data.publishedAtMs !== undefined && (!Number.isFinite(data.publishedAtMs) || data.publishedAtMs < 0))
            return false;
        return true;
    }
    function accept(data) {
        if (!validate(data)) {
            error = "Wallpaper presentation data is invalid; the previous pair is preserved.";
            return false;
        }
        // A delayed filesystem read must not undo a newer streamed publication.
        if ((data.publishedAtMs || 0) < (pending.publishedAtMs || 0))
            return false;
        const record = JSON.parse(JSON.stringify(data));
        record.poster = canonicalPoster(record.poster);
        if (JSON.stringify(record) === JSON.stringify(pending)) {
            error = "";
            return true;
        }
        const commit = !active.colours || active.poster === record.poster;
        state = {
            pending: record,
            active: commit ? record : active
        };
        revision++;
        error = "";
        return true;
    }
    function activate(poster) {
        const path = canonicalPoster(poster);
        if (!path || path !== pending.poster || !pending.colours)
            return false;
        if (active !== pending) {
            state = {
                pending: pending,
                active: pending
            };
            revision++;
        }
        error = "";
        return true;
    }
    function load(text) {
        try {
            if (text.length > 2097152)
                throw new Error();
            return accept(JSON.parse(text));
        } catch (failure) {
            error = "Wallpaper presentation could not be read; the previous pair is preserved.";
            return false;
        }
    }
    FileView {
        path: NacrePaths.state + "/presentation.json"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: root.load(text())
        onLoadFailed: root.error = "Wallpaper presentation is not available yet."
    }
    IpcHandler {
        target: "presentationState"
        function state(): string {
            return JSON.stringify({
                available: root.available,
                activeReady: !!root.active.colours,
                pendingMatchesActive: root.pending.poster === root.active.poster,
                revision: root.revision,
                error: root.error,
                mode: root.active.mode || ""
            });
        }
    }
}
