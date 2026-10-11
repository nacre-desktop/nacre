import qs.widgets
import qs.services
import QtQuick
import QtQuick.Controls
import Quickshell

Item {
    id: root
    required property PersistentProperties visibilities
    function captureUiState() {
        transcript.savePosition();
        return {
            position: transcript.savedPosition,
            cursor: input.cursorPosition,
            threadId: SidebarChat.threadId,
            composerKey: SidebarChat.composerKey,
            draft: SidebarChat.draft,
            attachments: SidebarChat.attachments
        };
    }
    property var pendingRecovery: null
    function restoreUiState(data) {
        SidebarChat.restoreComposer(data);
        pendingRecovery = data;
        if (!data.threadId || (SidebarChat.threadId === data.threadId && SidebarChat.messages.count > 0))
            finishRecovery();
    }
    function finishRecovery() {
        const data = pendingRecovery;
        if (!data)
            return;
        pendingRecovery = null;
        if (data.threadId && SidebarChat.threadId && data.threadId !== SidebarChat.threadId)
            return;
        transcript.savedPosition = data.position;
        transcript.restoring = true;
        transcript.restorePosition();
        input.cursorPosition = Math.min(data.cursor || 0, input.text.length);
    }
    function codeBlocks(text) {
        const result = [];
        const pattern = /```[^\n]*\n([\s\S]*?)```/g;
        let match;
        while ((match = pattern.exec(text)) !== null)
            result.push(match[1]);
        return result;
    }
    property bool active: false
    onActiveChanged: if (active) {
        SidebarChat.start();
        if (transcript.follow)
            transcript.latest();
    }
    Component.onCompleted: transcript.latest()
    Column {
        id: tools
        anchors.top: parent.top
        width: parent.width
        spacing: 7
        Row {
            spacing: 8
            ActionButton {
                text: "New chat"
                icon: "add_comment"
                enabled: !SidebarChat.busy
                onClicked: SidebarChat.newChat()
            }
            ActionButton {
                text: "Workspace"
                icon: "open_in_new"
                enabled: !SidebarChat.busy && SidebarChat.threadId.length > 0
                onClicked: SidebarChat.workspace()
            }
        }
        NacreText {
            width: parent.width
            elide: Text.ElideRight
            text: (SidebarChat.provider ? SidebarChat.provider.charAt(0).toUpperCase() + SidebarChat.provider.slice(1) : "Assistant") + " · " + SidebarChat.title
            color: NacreColours.palette.m3onSurfaceVariant
            font.pointSize: 10
        }
    }
    ListView {
        id: transcript
        objectName: "sidebarTranscript"
        anchors.top: tools.bottom
        anchors.bottom: footer.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.topMargin: 12
        anchors.bottomMargin: 10
        model: SidebarChat.messages
        clip: true
        spacing: 12
        cacheBuffer: 100
        ScrollBar.vertical: ScrollBar {
            onPressedChanged: {
                if (pressed) {
                    wheel.cancel();
                    transcript.follow = false;
                } else if (!transcript.restoring)
                    transcript.follow = transcript.nearBottom();
            }
        }
        FastScroll {
            id: wheel
            objectName: "sidebarWheel"
            view: transcript
            onScrolled: transcript.follow = false
            onSettled: if (!transcript.restoring)
                transcript.follow = transcript.nearBottom()
        }
        boundsBehavior: Flickable.StopAtBounds
        maximumFlickVelocity: 4000
        flickDeceleration: 6500
        property bool follow: true
        property bool restoring: false
        property var savedPosition: ({})
        function nearBottom() {
            return contentHeight <= height || originY + contentHeight - height - contentY < 48;
        }
        function latest() {
            if (wheel)
                wheel.cancel();
            follow = true;
            Qt.callLater(() => {
                if (follow && !restoring) {
                    forceLayout();
                    positionViewAtEnd();
                }
            });
        }
        function savePosition() {
            const bottom = follow || nearBottom();
            let index = indexAt(12, contentY + 1);
            if (index < 0)
                index = indexAt(12, contentY + 16);
            const item = index >= 0 ? itemAtIndex(index) : null;
            savedPosition = {
                bottom: bottom,
                id: index >= 0 ? SidebarChat.messages.get(index).id : "",
                offset: item ? contentY - item.y : 0,
                y: contentY - originY
            };
            restoring = true;
            wheel.cancel();
        }
        function restorePosition() {
            Qt.callLater(() => {
                forceLayout();
                follow = !!savedPosition.bottom;
                if (follow)
                    positionViewAtEnd();
                else {
                    const index = SidebarChat.itemIndex(savedPosition.id);
                    if (index >= 0) {
                        positionViewAtIndex(index, ListView.Beginning);
                        forceLayout();
                        const item = itemAtIndex(index);
                        if (item)
                            contentY = item.y + savedPosition.offset;
                    } else
                        contentY = originY + Math.min(savedPosition.y, Math.max(0, contentHeight - height));
                }
                restoring = false;
                if (follow)
                    Qt.callLater(() => {
                        if (follow && !restoring) {
                            forceLayout();
                            positionViewAtEnd();
                        }
                    });
            });
        }
        onDraggingChanged: if (dragging)
            follow = false
        onContentHeightChanged: if (follow && !restoring)
            Qt.callLater(() => {
                if (follow && !restoring) {
                    forceLayout();
                    positionViewAtEnd();
                }
            })
        onHeightChanged: if (follow && !restoring)
            latest()
        delegate: NacreSurface {
            id: bubble
            required property string role
            required property string text
            width: transcript.width
            implicitHeight: message.implicitHeight + 45 + copyActions.implicitHeight
            radius: 13
            color: role === "user" ? NacreColours.palette.m3secondaryContainer : NacreColours.palette.m3surfaceContainer
            NacreText {
                x: 12
                y: 9
                text: bubble.role === "user" ? "You" : "Nacre AI"
                font.pointSize: 9
                color: NacreColours.palette.m3primary
            }
            Flow {
                id: copyActions
                anchors.top: message.bottom
                anchors.topMargin: 5
                anchors.left: parent.left
                anchors.right: parent.right
                spacing: 5
                ActionButton {
                    text: "Copy"
                    onClicked: SidebarChat.copy(bubble.text)
                }
                Repeater {
                    model: root.codeBlocks(bubble.text)
                    ActionButton {
                        required property string modelData
                        text: "Copy code"
                        onClicked: SidebarChat.copy(modelData)
                    }
                }
            }
            TextEdit {
                id: message
                x: 12
                y: 29
                width: parent.width - 24
                text: bubble.text
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                textFormat: bubble.role === "assistant" ? TextEdit.MarkdownText : TextEdit.PlainText
                color: NacreColours.palette.m3onSurface
                font.family: "IBM Plex Sans"
                font.pointSize: 12
                selectionColor: NacreColours.palette.m3primary
                selectedTextColor: NacreColours.palette.m3onPrimary
            }
        }
        NacreText {
            anchors.centerIn: parent
            visible: transcript.count === 0
            text: "Start a conversation"
            color: NacreColours.palette.m3onSurfaceVariant
        }
    }
    Connections {
        target: SidebarChat
        function onHistoryReplacing() {
            transcript.savePosition();
        }
        function onHistoryReplaced() {
            if (root.pendingRecovery)
                root.finishRecovery();
            else
                transcript.restorePosition();
        }
        function onThreadIdChanged() {
            if (root.pendingRecovery && root.pendingRecovery.threadId === SidebarChat.threadId)
                return;
            transcript.savedPosition = {
                bottom: true
            };
            transcript.latest();
        }
    }
    DropArea {
        anchors.fill: parent
        enabled: SidebarChat.attachmentsSupported
        onDropped: drop => {
            if (drop.hasUrls) {
                SidebarChat.addFiles(drop.urls);
                drop.acceptProposedAction();
            }
        }
    }
    Column {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width
        spacing: 8
        NacreText {
            width: parent.width
            wrapMode: Text.Wrap
            visible: SidebarChat.error.length > 0 || SidebarChat.status.length > 0
            text: SidebarChat.error || SidebarChat.status
            color: SidebarChat.error ? NacreColours.palette.m3error : NacreColours.palette.m3onSurfaceVariant
            font.pointSize: 10
        }
        ScrollView {
            id: questionScroll
            FastScroll {
                view: questionScroll.contentItem
                parent: questionScroll.contentItem
            }
            objectName: "sidebarQuestions"
            width: parent.width
            height: visible ? Math.min(240, root.height * 0.35, Math.max(80, questionColumn.implicitHeight)) : 0
            visible: SidebarChat.question !== null
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            Column {
                id: questionColumn
                width: questionScroll.availableWidth
                spacing: 10
                Repeater {
                    model: SidebarChat.question?.questions ?? []
                    Column {
                        id: q
                        required property var modelData
                        width: parent.width
                        spacing: 6
                        NacreText {
                            width: parent.width
                            wrapMode: Text.Wrap
                            text: q.modelData.question
                        }
                        Flow {
                            width: parent.width
                            spacing: 5
                            Repeater {
                                model: q.modelData.options ?? []
                                ActionButton {
                                    required property var modelData
                                    text: modelData.label
                                    selected: SidebarChat.answers[q.modelData.id]?.answers?.[0] === text
                                    onClicked: SidebarChat.setAnswer(q.modelData.id, text)
                                }
                            }
                        }
                        NacreTextField {
                            objectName: "questionAnswer-" + q.modelData.id
                            width: parent.width
                            height: 35
                            placeholderText: "Your answer"
                            leftPadding: 10
                            background: NacreSurface {
                                radius: 10
                                color: NacreColours.palette.m3surfaceContainerHigh
                            }
                            onTextChanged: SidebarChat.setAnswer(q.modelData.id, text)
                        }
                    }
                }
                ActionButton {
                    objectName: "sidebarReply"
                    text: "Reply"
                    enabled: !SidebarChat.inWorkspace && SidebarChat.question !== null && SidebarChat.question.questions.every(q => (SidebarChat.answers[q.id]?.answers?.[0] ?? "").trim().length > 0)
                    onClicked: SidebarChat.answer()
                }
            }
        }
        Flow {
            width: parent.width
            spacing: 5
            Repeater {
                model: SidebarChat.attachments ?? []
                ActionButton {
                    required property var modelData
                    text: modelData.name
                    icon: "close"
                    onClicked: SidebarChat.removeAttachment(modelData.path)
                }
            }
        }
        ScrollView {
            id: composer
            FastScroll {
                view: composer.contentItem
                parent: composer.contentItem
            }
            objectName: "sidebarComposerScroll"
            width: parent.width
            height: Math.min(160, Math.max(100, input.implicitHeight))
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            TextArea {
                id: input
                objectName: "sidebarComposer"
                enabled: !SidebarChat.inWorkspace
                width: composer.availableWidth
                wrapMode: TextEdit.Wrap
                text: SidebarChat.draft
                onTextChanged: if (SidebarChat.draft !== text)
                    SidebarChat.draft = text
                placeholderText: SidebarChat.inWorkspace ? "Chat moved to the workspace" : SidebarChat.busy ? "Add a message while I work" : "Message Nacre AI"
                color: NacreColours.palette.m3onSurface
                placeholderTextColor: NacreColours.palette.m3onSurfaceVariant
                selectionColor: NacreColours.palette.m3primary
                selectedTextColor: NacreColours.palette.m3onPrimary
                font.family: "IBM Plex Sans"
                font.pointSize: 12
                background: NacreSurface {
                    radius: 14
                    color: NacreColours.palette.m3surfaceContainerHigh
                }
                Keys.onReturnPressed: event => {
                    if (event.modifiers & Qt.ShiftModifier) {
                        event.accepted = false;
                    } else {
                        root.send();
                        event.accepted = true;
                    }
                }
            }
        }
        Connections {
            target: SidebarChat
            function onDraftChanged() {
                if (input.text !== SidebarChat.draft)
                    input.text = SidebarChat.draft;
            }
        }
        Row {
            objectName: "sidebarActions"
            width: parent.width
            spacing: 6
            ActionButton {
                objectName: "sidebarSend"
                compact: true
                text: "Send"
                icon: "arrow_upward"
                selected: true
                enabled: !SidebarChat.inWorkspace && (input.text.trim().length > 0 || (SidebarChat.attachments ?? []).length > 0)
                onClicked: root.send()
            }
            ActionButton {
                compact: true
                text: "Attach"
                icon: "attach_file"
                enabled: SidebarChat.attachmentsSupported
                onClicked: SidebarChat.pickFiles()
            }
            ActionButton {
                compact: true
                text: SidebarChat.busy ? "" : "Screenshot"
                icon: "screenshot"
                enabled: SidebarChat.attachmentsSupported
                onClicked: SidebarChat.screenshot()
            }
            ActionButton {
                compact: true
                text: ""
                icon: "stop"
                visible: SidebarChat.busy
                onClicked: SidebarChat.stop()
            }
            ActionButton {
                objectName: "sidebarLatest"
                compact: true
                text: "Latest"
                icon: "arrow_downward"
                onClicked: transcript.latest()
            }
        }
    }
    function send() {
        if (!SidebarChat.inWorkspace && (input.text.trim() || (SidebarChat.attachments ?? []).length)) {
            transcript.latest();
            if (SidebarChat.send(input.text) === false)
                return;
            SidebarChat.draft = "";
            SidebarChat.attachments = [];
            input.text = "";
        }
    }
}
