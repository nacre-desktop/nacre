.pragma library

// The daemon's compositor topic is the authoritative collection snapshot. A
// stock niri topic omits both the capability and the rows.
function fromDaemonState(state) {
    const supported = state?.backend === "niri" && state?.capabilities?.workspaceCollections === true;
    return {
        supported: supported,
        rows: supported && Array.isArray(state.collections) ? state.collections : [],
        activeId: supported && typeof state.activeCollection === "string" ? state.activeCollection : ""
    };
}

// Keep the Niri adapter's accepted snapshot shape in one place so topic and
// event fixtures exercise the exact state update that drives the top bar.
function acceptSnapshot(target, snapshot) {
    const supported = snapshot?.capabilities?.workspaceCollections === true;
    target.collectionsSupported = supported;
    target.collections = supported && Array.isArray(snapshot.collections) ? snapshot.collections : [];
    target.activeCollectionId = supported && typeof snapshot.activeCollection === "string" ? snapshot.activeCollection : "";
}
