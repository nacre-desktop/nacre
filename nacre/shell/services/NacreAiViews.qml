pragma Singleton
import QtQuick
import Quickshell

Singleton {
    id: root
    property var edge: SidebarChat
    property var sessions: [SidebarChat]
    property var parked: []
    property int counter: 0
    readonly property var recent: parked.filter(model => meaningful(model)).map(model => ({
                model: model,
                title: model.title || NacreI18n.tr("ui.ai-views.title.draft", "Draft"),
                threadId: model.threadId,
                busy: model.busy
            }))
    Component {
        id: sessionFactory
        NacreChatSession {}
    }
    function meaningful(model) {
        return !!(model.threadId || model.busy || model.draft.trim() || model.attachments?.length || model.messages?.count || model.outgoing?.length);
    }
    function retained(model) {
        return model === edge || meaningful(model) || model.composerBusy || model.recoveryPending || NacrePearls.entries.some(entry => entry.chat === model);
    }
    function fresh(savedChannel = "") {
        const channel = savedChannel || "p" + Qt.md5(Date.now().toString() + Math.random()).slice(0, 24);
        const model = sessionFactory.createObject(root, {
            channel: channel
        });
        sessions = [...sessions, model];
        return model;
    }
    function take() {
        const model = edge;
        model.saveDraft();
        edge = fresh();
        NacrePearls.saveSoon();
        return model;
    }
    function dock(model) {
        if (edge !== model) {
            edge.saveDraft();
            edge.parked = true;
            if (!parked.includes(edge))
                parked = [...parked, edge];
            parked = parked.filter(value => value !== model);
            edge = model;
            model.parked = false;
            model.start();
            NacrePearls.saveSoon();
        }
    }
    function park(model) {
        model.saveDraft();
        model.parked = true;
        if (edge !== model && !parked.includes(model))
            parked = [...parked, model];
    }
    function restore(value) {
        const channels = Array.isArray(value.channels) ? value.channels : [""];
        for (const channel of channels) {
            if (channel && /^[a-z0-9-]{1,48}$/.test(channel) && !sessions.some(model => model.channel === channel))
                fresh(channel);
        }
        edge = sessions.find(model => model.channel === (value.edge || "")) || SidebarChat;
        parked = sessions.filter(model => model !== edge);
        for (const model of sessions) {
            model.parked = model !== edge;
            model.recoveryPending = channels.includes(model.channel);
            if (channels.length > 1)
                model.start();
        }
    }
    function snapshot() {
        return {
            edge: edge.channel,
            channels: sessions.filter(model => retained(model)).map(model => model.channel)
        };
    }
    function select(model) {
        const owned = NacrePearls.entries.find(entry => entry.kind === "ai" && entry.chat === model);
        if (owned) {
            owned.window?.activate();
            return;
        }
        dock(model);
    }
    function load(key) {
        const metadata = DesktopExtras.chats.find(item => item.key === key);
        const model = metadata ? sessions.find(item => item.threadId === metadata.id && item.provider === (metadata.agent || "codex") && (item.account || "Default") === (metadata.account || "Default")) : null;
        if (model) {
            select(model);
            return;
        }
        edge.load(key);
    }
}
