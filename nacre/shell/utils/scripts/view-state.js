.pragma library

// Only UI state is serialized. Password controls and rendered note/chat text are excluded.
function capture(root) {
    const saved = {};
    function visit(item, path) {
        if (!item)
            return;
        if (path !== "root" && typeof item.captureUiState === "function") {
            saved[path] = {
                custom: item.captureUiState()
            };
            return;
        }
        const state = {};
        for (const key of ["section", "captureOpen", "category", "paletteGroup"])
            if (typeof item[key] === "string" || typeof item[key] === "boolean")
                state[key] = item[key];
        if (typeof item.contentY === "number" && item.interactive !== false) {
            state.y = item.contentY - (item.originY || 0);
            state.x = item.contentX - (item.originX || 0);
        }
        if (typeof item.cursorPosition === "number" && item.readOnly !== true && (item.echoMode === undefined || item.echoMode === 0)) {
            state.text = String(item.text || "").slice(0, 100000);
            state.cursor = item.cursorPosition;
            state.selectionStart = item.selectionStart;
            state.selectionEnd = item.selectionEnd;
        }
        if (Object.keys(state).length)
            saved[path] = state;
        const children = item.children || [];
        for (let i = 0; i < children.length; i++)
            visit(children[i], path + "/" + (children[i].objectName || i));
    }
    visit(root, "root");
    return saved;
}

function restore(root, saved) {
    function visit(item, path) {
        if (!item)
            return;
        const state = saved[path];
        if (state) {
            if (state.custom && typeof item.restoreUiState === "function") {
                item.restoreUiState(state.custom);
                return;
            }
            for (const key of ["section", "captureOpen", "category", "paletteGroup"])
                if (state[key] !== undefined)
                    item[key] = state[key];
            if (state.text !== undefined && typeof item.cursorPosition === "number" && item.readOnly !== true && (item.echoMode === undefined || item.echoMode === 0)) {
                item.text = state.text;
                item.cursorPosition = Math.min(state.cursor, item.text.length);
                if (typeof item.select === "function" && state.selectionStart !== undefined)
                    item.select(state.selectionStart, state.selectionEnd);
            }
            if (state.y !== undefined) {
                if (typeof item.forceLayout === "function")
                    item.forceLayout();
                item.contentY = (item.originY || 0) + Math.max(0, Math.min(state.y, item.contentHeight - item.height));
                item.contentX = (item.originX || 0) + Math.max(0, Math.min(state.x, item.contentWidth - item.width));
            }
        }
        const children = item.children || [];
        for (let i = 0; i < children.length; i++)
            visit(children[i], path + "/" + (children[i].objectName || i));
    }
    visit(root, "root");
}
