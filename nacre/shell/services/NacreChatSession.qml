import Quickshell
import Quickshell.Io
import QtQuick
import qs.utils

Scope {
    id: root
    property string channel: ""
    property bool recoveryPending: false
    property bool parked: false
    onParkedChanged: idleDisconnect.restart()
    onBusyChanged: idleDisconnect.restart()
    Timer {
        id: idleDisconnect
        interval: 3000
        onTriggered: if (root.parked && !root.busy) {
            root.wantedConnection = false;
            wire.running = false;
        }
    }
    property string provider: ""
    property string defaultProvider: "codex"
    property bool inWorkspace: false
    property string account: ""
    property string title: NacreI18n.tr("ui.chat-session.title.new-chat", "New chat")
    property string threadId: ""
    property string model: ""
    property bool busy: false
    property bool connected: false
    property bool wantedConnection: false
    property string status: ""
    property string error: ""
    property var question: null
    property var answers: ({})
    property var outgoing: []
    property string draft: ""
    property var attachments: []
    property bool attachmentsSupported: false
    property string composerKey: ""
    property bool loadingDraft: false
    property int draftRevision: 0
    property var composerQueue: []
    property var recoveryComposer: null
    function restoreComposer(data) {
        if (!data.composerKey)
            return;
        recoveryComposer = data;
        composer("save", {
            key: data.composerKey,
            text: data.draft || "",
            attachments: data.attachments || []
        });
        applyRecoveryComposer();
    }
    function applyRecoveryComposer() {
        const data = recoveryComposer;
        if (!data || data.composerKey !== composerKey)
            return;
        loadingDraft = true;
        draft = data.draft || "";
        attachments = data.attachments || [];
        draftRevision++;
        loadingDraft = false;
        recoveryComposer = null;
    }
    function composer(action, payload) {
        composerQueue.push({
            action: action,
            payload: payload ?? {},
            contextKey: composerKey
        });
        nextComposer();
    }
    function nextComposer() {
        if (composerWorker.running || composerQueue.length === 0)
            return;
        let request = composerQueue.shift();
        while (["files", "pick", "screenshot"].includes(request.action) && request.contextKey !== composerKey) {
            if (!composerQueue.length)
                return;
            request = composerQueue.shift();
        }
        composerWorker.contextKey = request.contextKey;
        composerWorker.action = request.action;
        composerWorker.payload = request.payload;
        composerWorker.command = [NacrePaths.tools + "/nacre-helper", "composer", request.action];
        composerWorker.running = true;
    }
    function saveDraft() {
        if (composerKey && !loadingDraft)
            composer("save", {
                key: composerKey,
                text: draft,
                attachments: attachments
            });
    }
    onDraftChanged: if (!loadingDraft) {
        draftRevision++;
        draftSave.restart();
    }
    onAttachmentsChanged: if (!loadingDraft)
        draftSave.restart()
    function removeAttachment(path) {
        attachments = attachments.filter(f => f.path !== path);
    }
    function addFiles(files) {
        if (attachmentsSupported)
            composer("files", {
                files: files
            });
    }
    function pickFiles() {
        if (attachmentsSupported)
            composer("pick");
    }
    function screenshot() {
        if (attachmentsSupported)
            composer("screenshot");
    }
    function copy(text) {
        composer("copy", {
            text: text
        });
    }
    Timer {
        id: draftSave
        interval: 250
        onTriggered: root.saveDraft()
    }
    Process {
        id: composerWorker
        objectName: "composerWorker"
        stdinEnabled: true
        property string action
        property string contextKey: ""
        property var payload: ({})
        onStarted: {
            if (["load", "save", "files", "copy"].includes(action)) {
                write(JSON.stringify(payload) + "\n");
            }
        }
        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    const value = JSON.parse(line);
                    if (value.error)
                        root.error = value.error;
                    else if (composerWorker.action === "load" && composerWorker.payload.key === root.composerKey && composerWorker.payload.revision === root.draftRevision) {
                        root.loadingDraft = true;
                        root.draft = value.text ?? "";
                        root.attachments = value.attachments ?? [];
                        root.loadingDraft = false;
                        root.recoveryPending = false;
                    } else if (["files", "pick", "screenshot"].includes(composerWorker.action) && composerWorker.contextKey === root.composerKey && value.attachments) {
                        const unique = new Map();
                        for (const file of [...root.attachments, ...value.attachments])
                            unique.set(file.path, file);
                        root.attachments = [...unique.values()];
                    }
                } catch (e) {
                    root.error = NacreI18n.tr("ui.chat-session.error.could-not-update-the-draft", "Could not update the draft");
                }
            }
        }
        onExited: root.nextComposer()
    }
    signal historyReplacing
    signal historyReplaced
    readonly property bool composerBusy: composerWorker.running
    property alias messages: messages
    ListModel {
        id: messages
        dynamicRoles: true
    }
    function start() {
        if (!NacreProfile.ai) {
            error = NacreI18n.tr("ui.chat-session.error.nacre-ai-is-optional-enable-it-through-component-setup", "Nacre AI is optional. Enable it through component setup.");
            return;
        }
        wantedConnection = true;
        if (!wire.running)
            wire.running = true;
    }
    Timer {
        interval: 1000
        repeat: true
        running: root.wantedConnection && !root.inWorkspace && !wire.running
        onTriggered: wire.running = true
    }
    function command(value) {
        start();
        if (connected)
            wire.write(JSON.stringify(value) + "\n");
        else
            outgoing.push(value);
    }
    function send(text) {
        if (inWorkspace || (!text.trim() && attachments.length === 0))
            return false;
        if (attachments.length && !attachmentsSupported) {
            error = NacreI18n.tr("ui.chat-session.error.reconnect-the-assistant-before-sending-attachments", "Reconnect the assistant before sending attachments");
            return false;
        }
        busy = true;
        command({
            action: "send",
            text: text || NacreI18n.tr("ui.chat-session.text.please-review-the-attached-files", "Please review the attached files."),
            attachments: attachments
        });
        return true;
    }
    function newChat() {
        if (!busy)
            command({
                action: "new"
            });
    }
    function load(key) {
        if (!busy)
            command({
                action: "load",
                key: key
            });
    }
    function stop() {
        command({
            action: "stop"
        });
    }
    function setProvider(provider) {
        command({
            action: "provider",
            provider: provider
        });
    }
    function workspace() {
        command({
            action: "workspace"
        });
    }
    function answer() {
        if (question)
            command({
                action: "answer",
                id: question.id,
                answers: answers
            });
    }
    function setAnswer(id, text) {
        const result = Object.assign({}, answers);
        result[id] = {
            answers: [text]
        };
        answers = result;
    }
    function itemIndex(id) {
        for (let i = 0; i < messages.count; i++)
            if (messages.get(i).id === id)
                return i;
        return -1;
    }
    function accept(event) {
        if (event.type === "connection") {
            connected = event.connected;
            error = event.error ?? "";
            return;
        }
        if (event.type === "state") {
            attachmentsSupported = (event.features ?? []).includes("attachments");
            const key = event.composerKey ?? event.threadId ?? "";
            if (key && key !== composerKey) {
                const carry = !composerKey && (draft.length > 0 || attachments.length > 0);
                const typed = draft, files = attachments;
                saveDraft();
                loadingDraft = true;
                composerKey = key;
                draft = carry ? typed : "";
                attachments = carry ? files : [];
                loadingDraft = false;
                applyRecoveryComposer();
                if (carry)
                    saveDraft();
                else
                    composer("load", {
                        key: key,
                        revision: draftRevision
                    });
            }
            defaultProvider = event.defaultProvider ?? defaultProvider;
            inWorkspace = event.inWorkspace ?? false;
            provider = event.provider ?? provider;
            account = event.account ?? account;
            title = event.title ?? title;
            threadId = event.threadId ?? "";
            model = event.model ?? "";
            busy = event.busy ?? false;
            status = event.status ?? "";
            error = event.error ?? "";
            if (question?.id !== event.question?.id)
                answers = {};
            question = event.question ?? null;
            connected = true;
            while (outgoing.length)
                wire.write(JSON.stringify(outgoing.shift()) + "\n");
        } else if (event.type === "sendFailed") {
            draft = event.text + (draft ? "\n\n" + draft : "");
            if (event.attachments)
                attachments = event.attachments;
        } else if (event.type === "history") {
            historyReplacing();
            messages.clear();
            for (const item of event.messages)
                messages.append(item);
            historyReplaced();
        } else if (event.type === "delta") {
            const index = itemIndex(event.id);
            if (index < 0)
                messages.append({
                    id: event.id,
                    role: "assistant",
                    text: event.text
                });
            else
                messages.setProperty(index, "text", messages.get(index).text + event.text);
        } else if (event.type === "message") {
            const index = itemIndex(event.id);
            if (index < 0)
                messages.append({
                    id: event.id,
                    role: event.role,
                    text: event.text
                });
            else
                messages.setProperty(index, "text", event.text);
        }
    }
    Process {
        id: wire
        stdinEnabled: true
        command: ["python3", NacrePaths.pearlTools + "/sidebar-chat.py", "client"].concat(root.channel ? ["--channel", root.channel] : [])
        stdout: SplitParser {
            onRead: line => {
                try {
                    root.accept(JSON.parse(line));
                } catch (e) {
                    root.error = NacreI18n.tr("ui.chat-session.error.could-not-read-the-assistant-response", "Could not read the assistant response");
                }
            }
        }
        onExited: {
            root.connected = false;
            if (root.busy) {
                root.error = NacreI18n.tr("ui.chat-session.error.reopen-the-drawer-to-reconnect-to-the-assistant", "Reopen the drawer to reconnect to the assistant");
                root.busy = false;
            }
        }
    }
}
