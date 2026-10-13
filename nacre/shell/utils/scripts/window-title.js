function caption(title, appId, chatTitle) {
    const value = String(title || "");
    if (appId !== "nacre-ai-task")
        return value;
    const spinner = /^[\u2800-\u28ff]\s*/.exec(value)?.[0] || "";
    // The agent's terminal title appends its chat directory after a separator.
    // Keep ordinary title punctuation; remove only a path/identifier suffix.
    const name = value.replace(/^[\u2800-\u28ff]\s*/, "").replace(/\s+\|\s+(?:[0-9a-f]{12,}|\/[\s\S]*|~\/[\s\S]*)[\s\S]*$/i, "");
    return spinner + (chatTitle || name);
}

function displayWindow(active, windows, workspaceId, previousId, backend) {
    if (active)
        return active;
    if (backend !== "niri")
        return null;
    // Niri briefly clears keyboard focus for its screenshot UI. Keep the
    // caption only while that window still exists on the current workspace.
    return windows.find(window => window.windowId === previousId && window.workspaceId === workspaceId) || null;
}
