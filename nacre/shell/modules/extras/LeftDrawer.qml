import qs.widgets
import qs.services
import qs.config
import Quickshell
import Quickshell.Io
import QtQuick
import QtQuick.Controls

Item {
    id: root
    property var chat: NacreAiViews.edge
    required property PersistentProperties visibilities
    required property ShellScreen screen
    function prepareLift() {
        edgeOpening.complete();
    }
    property bool handoffHidden: false
    opacity: handoffHidden ? 0 : 1
    onEnabledChanged: if (enabled)
        handoffHidden = false
    property bool detached: false
    property bool hostActive: true
    property bool captureOpen: false
    property string section: "chat"
    property real refreshedAt: 0
    visible: width > 0
    clip: true
    implicitWidth: detached ? parent.width : visibilities.left ? 470 : 0
    implicitHeight: detached ? parent.height : Math.min(810, screen.height - 150)
    Behavior on implicitWidth {
        enabled: !root.detached
        NumberAnimation {
            id: edgeOpening
            duration: NacreAppearance.anim.durations.normal
            easing.type: Easing.InOutCubic
        }
    }
    function refreshChats() {
        if (section === "chats" && visibilities.left && Date.now() - refreshedAt > 60000) {
            refreshedAt = Date.now();
            metadataDelay.restart();
        }
    }
    onSectionChanged: refreshChats()
    Connections {
        target: root.visibilities
        function onLeftChanged() {
            root.refreshChats();
        }
    }
    Timer {
        id: metadataDelay
        interval: 450
        onTriggered: DesktopExtras.request("chats", {})
    }
    Item {
        id: heading
        x: 16
        y: 16
        visible: true
        width: Math.max(0, root.width - 32)
        height: 34
        BrandLogo {
            id: logo
            ai: true
            implicitWidth: 30
            implicitHeight: 30
            anchors.verticalCenter: parent.verticalCenter
        }
        NacreText {
            text: "Nacre AI"
            anchors.left: logo.right
            anchors.leftMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            font.pointSize: 17
            color: NacreColours.palette.m3primary
        }
        ActionButton {
            visible: !root.detached
            anchors.right: parent.right
            text: ""
            icon: "push_pin"
            selected: root.visibilities.leftPinned
            onClicked: root.visibilities.leftPinned = !selected
        }
    }
    Row {
        id: tabs
        x: 16
        anchors.top: heading.bottom
        anchors.topMargin: 12
        spacing: 6
        ActionButton {
            text: "Chat"
            icon: "forum"
            selected: root.section === "chat"
            onClicked: root.section = "chat"
        }
        ActionButton {
            text: "Chats"
            icon: "history"
            selected: root.section === "chats"
            onClicked: root.section = "chats"
        }
        ActionButton {
            text: "Brain"
            icon: "neurology"
            selected: root.section === "brain"
            onClicked: root.section = "brain"
        }
        ActionButton {
            text: ""
            icon: "settings"
            selected: root.section === "settings"
            onClicked: {
                root.chat.start();
                root.section = "settings";
            }
        }
    }
    function captureState() {
        const ui = chatPane.captureUiState();
        delete ui.draft;
        delete ui.attachments;
        return {
            section: section,
            chatUi: ui
        };
    }
    function restoreState(value) {
        section = value.section || "chat";
        if (value.chatUi)
            chatPane.restoreUiState(value.chatUi, false);
    }
    ChatPane {
        id: chatPane
        anchors.top: tabs.bottom
        anchors.topMargin: 14
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 16
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        visibilities: root.visibilities
        chat: root.chat
        active: root.hostActive && root.visibilities.left && root.section === "chat"
        visible: root.section === "chat"
    }
    Flickable {
        id: knowledge
        x: 16
        width: Math.max(0, root.width - 32)
        anchors.top: tabs.bottom
        anchors.topMargin: 14
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 16
        visible: root.section !== "chat"
        contentWidth: width
        contentHeight: content.implicitHeight
        clip: true
        ScrollBar.vertical: ScrollBar {}
        FastScroll {
            view: knowledge
        }
        Column {
            id: content
            width: Math.max(0, root.width - 32)
            spacing: 12
            Column {
                width: Math.max(0, root.width - 32)
                spacing: 12
                visible: root.section === "settings"
                NacreText {
                    text: "Default assistant"
                    color: NacreColours.palette.m3primary
                }
                Row {
                    spacing: 8
                    ActionButton {
                        text: "Codex"
                        selected: root.chat.defaultProvider === "codex"
                        onClicked: root.chat.setProvider("codex")
                    }
                    ActionButton {
                        text: "Claude Code"
                        selected: root.chat.defaultProvider === "claude"
                        onClicked: root.chat.setProvider("claude")
                    }
                }
                NacreText {
                    width: Math.max(0, root.width - 32)
                    wrapMode: Text.Wrap
                    text: "Used for new chats here and in the workspace. Existing chats keep their assistant."
                    font.pointSize: 11
                    color: NacreColours.palette.m3onSurfaceVariant
                }
                ActionButton {
                    text: "New chat"
                    icon: "add_comment"
                    enabled: !root.chat.busy
                    onClicked: {
                        root.chat.newChat();
                        root.section = "chat";
                    }
                }
                ActionButton {
                    text: "Accounts and usage"
                    icon: "manage_accounts"
                    onClicked: DesktopActions.execute("tasks")
                }
            }
            NacreTextField {
                id: search
                visible: root.section === "brain"
                width: Math.max(0, root.width - 32)
                height: 42
                leftPadding: 13
                rightPadding: 13
                placeholderText: "Search your brain"
                text: DesktopExtras.brainQuery
                background: NacreSurface {
                    color: NacreColours.palette.m3surfaceContainerHigh
                    radius: 21
                }
                onTextChanged: {
                    DesktopExtras.brainQuery = text;
                    DesktopExtras.notes = [];
                    searchDelay.restart();
                }
                Connections {
                    target: DesktopExtras
                    function onBrainQueryChanged() {
                        if (search.text !== DesktopExtras.brainQuery)
                            search.text = DesktopExtras.brainQuery;
                    }
                }
                Timer {
                    id: searchDelay
                    interval: 250
                    onTriggered: DesktopExtras.request("brain", {
                        query: search.text
                    })
                }
            }
            Row {
                visible: root.section === "brain"
                spacing: 8
                ActionButton {
                    text: "Open brain"
                    icon: "neurology"
                    onClicked: DesktopActions.execute("brain")
                }
                ActionButton {
                    text: "Capture"
                    icon: "edit_note"
                    selected: root.captureOpen
                    onClicked: {
                        root.captureOpen = !selected;
                        if (root.captureOpen)
                            capture.forceActiveFocus();
                    }
                }
            }
            Column {
                width: Math.max(0, root.width - 32)
                spacing: 8
                visible: root.section === "brain" && root.captureOpen
                TextArea {
                    id: capture
                    width: Math.max(0, root.width - 32)
                    height: 105
                    placeholderText: "A thought worth keeping"
                    wrapMode: TextEdit.Wrap
                    color: NacreColours.palette.m3onSurface
                    placeholderTextColor: NacreColours.palette.m3onSurfaceVariant
                    selectionColor: NacreColours.palette.m3primary
                    selectedTextColor: NacreColours.palette.m3onPrimary
                    background: NacreSurface {
                        color: NacreColours.palette.m3surfaceContainerHigh
                        radius: 12
                    }
                }
                ActionButton {
                    text: "Save to brain"
                    icon: "save"
                    enabled: capture.text.trim().length > 0 && !DesktopExtras.busy.capture
                    onClicked: {
                        DesktopExtras.captured = "";
                        DesktopExtras.request("capture", {
                            text: capture.text
                        });
                    }
                }
                Connections {
                    target: DesktopExtras
                    function onCapturedChanged() {
                        if (DesktopExtras.captured) {
                            capture.text = "";
                            root.captureOpen = false;
                        }
                    }
                }
            }
            NacreText {
                width: Math.max(0, root.width - 32)
                wrapMode: Text.WordWrap
                visible: DesktopExtras.message.length > 0 || DesktopExtras.captured.length > 0
                text: DesktopExtras.message || DesktopExtras.captured
                color: NacreColours.palette.m3onSurfaceVariant
                font.pointSize: 11
            }
            Column {
                width: Math.max(0, root.width - 32)
                spacing: 8
                visible: root.section === "brain" && search.text.trim().length > 0
                NacreText {
                    text: DesktopExtras.busy.brain ? "Searching…" : "Knowledge"
                    color: NacreColours.palette.m3primary
                }
                Repeater {
                    model: DesktopExtras.notes
                    NacreSurface {
                        id: note
                        required property var modelData
                        width: Math.max(0, root.width - 32)
                        height: 115
                        radius: 13
                        color: NacreColours.palette.m3surfaceContainer
                        Column {
                            anchors.fill: parent
                            anchors.margins: 10
                            spacing: 5
                            NacreText {
                                width: 418
                                elide: Text.ElideRight
                                text: note.modelData.title
                                textFormat: Text.PlainText
                                font.pointSize: 11
                            }
                            NacreText {
                                width: 418
                                height: 32
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                text: note.modelData.preview
                                textFormat: Text.PlainText
                                font.pointSize: 9
                                color: NacreColours.palette.m3onSurfaceVariant
                            }
                            Row {
                                spacing: 8
                                ActionButton {
                                    text: "Read note"
                                    onClicked: DesktopExtras.request("note", {
                                        path: note.modelData.path
                                    })
                                }
                                ActionButton {
                                    text: "Explore"
                                    onClicked: {
                                        root.visibilities.left = false;
                                        root.visibilities.leftPinned = false;
                                        DesktopExtras.request("explore", {
                                            path: note.modelData.path
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                NacreText {
                    text: "No matching notes"
                    visible: DesktopExtras.notes.length === 0 && !DesktopExtras.busy.brain
                    color: NacreColours.palette.m3onSurfaceVariant
                }
            }
            Column {
                width: Math.max(0, root.width - 32)
                spacing: 8
                visible: root.section === "chats"
                NacreText {
                    text: "Workspace chats"
                    color: NacreColours.palette.m3primary
                }
                Repeater {
                    model: NacreHyprland.clients.filter(c => c.wmClass === "nacre-ai-task")
                    ChatCard {
                        required property var modelData
                        label: ChatWindowTitle.titles[modelData.pid] || modelData.title
                        detail: "Workspace " + modelData.workspace?.id
                        onClicked: {
                            root.visibilities.left = false;
                            root.visibilities.leftPinned = false;
                            NacreHyprland.dispatch('hl.dsp.focus({window=' + JSON.stringify('address:' + modelData.address) + '})');
                        }
                    }
                }
                Row {
                    spacing: 8
                    NacreText {
                        text: "Recent chats"
                        width: 195
                        anchors.verticalCenter: parent.verticalCenter
                        color: NacreColours.palette.m3primary
                    }
                    ActionButton {
                        text: "Refresh"
                        icon: "refresh"
                        enabled: !DesktopExtras.busy.chats
                        onClicked: DesktopExtras.request("chats", {})
                    }
                }
                NacreText {
                    visible: !!DesktopExtras.busy.chats
                    text: "Reading saved chats…"
                    font.pointSize: 10
                    color: NacreColours.palette.m3onSurfaceVariant
                }
                Repeater {
                    model: NacreAiViews.recent
                    ChatCard {
                        required property var modelData
                        label: modelData.title
                        detail: modelData.busy ? "Responding" : "Saved conversation or draft"
                        onClicked: {
                            NacreAiViews.select(modelData.model);
                            root.section = "chat";
                        }
                    }
                }
                Repeater {
                    model: DesktopExtras.chats.filter(c => !Object.values(ChatWindowTitle.threadIds).includes(c.id)).slice(0, 6)
                    ChatCard {
                        required property var modelData
                        label: modelData.title
                        detail: modelData.agent + " · " + modelData.account + " · " + modelData.state
                        onClicked: {
                            NacreAiViews.load(modelData.key);
                            root.section = "chat";
                        }
                    }
                }
                ActionButton {
                    text: "All saved chats"
                    icon: "forum"
                    onClicked: DesktopActions.execute("load")
                }
            }
        }
    }
    IpcHandler {
        target: root.detached ? "leftDrawerWindow" + root.visibilities.windowKey : "leftDrawer-" + root.screen.name
        function section(name: string): void {
            if (["chat", "chats", "brain", "settings"].includes(name))
                root.section = name;
        }
        function state(): string {
            return JSON.stringify({
                section: root.section,
                width: root.width,
                height: root.height,
                pinRight: heading.width
            });
        }
    }
    component ChatCard: NacreSurface {
        id: card
        property string label
        property string detail
        signal clicked
        width: Math.max(0, root.width - 32)
        height: 55
        radius: 12
        color: NacreColours.palette.m3surfaceContainer
        Column {
            anchors.fill: parent
            anchors.margins: 9
            spacing: 2
            NacreText {
                width: 418
                elide: Text.ElideRight
                text: card.label
                textFormat: Text.PlainText
                font.pointSize: 11
            }
            NacreText {
                text: card.detail
                font.pointSize: 9
                color: NacreColours.palette.m3onSurfaceVariant
            }
        }
        NacreInteraction {
            function onClicked() {
                card.clicked();
            }
        }
    }
}
