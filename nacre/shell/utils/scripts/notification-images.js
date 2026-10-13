// Notification senders may provide local images, not network fetch instructions.
function localSource(value) {
    if (typeof value !== "string")
        return "";
    if (value.startsWith("/") && !value.startsWith("//"))
        return "file://" + value;
    if (/^file:\/\/(?:\/|localhost\/)/i.test(value) || value.startsWith("image://") || value.startsWith("qrc:/"))
        return value;
    return "";
}

function iconName(value) {
    return typeof value === "string" && /^[A-Za-z0-9_.-]+$/.test(value) ? value : "";
}
