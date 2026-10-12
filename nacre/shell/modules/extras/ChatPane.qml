import qs.widgets
import qs.services
import QtQuick
import QtQuick.Controls
import Quickshell

Item {
    id: root
    property var chat: NacreAiViews.edge
    required property PersistentProperties visibilities
    function captureUiState() {
        transcript.savePosition();
        return {
            position: transcript.savedPosition,
            cursor: input.cursorPosition,
            threadId: root.chat.threadId,
            composerKey: root.chat.composerKey,
            draft: root.chat.draft,
            attachments: root.chat.attachments
        };
    }
    property var pendingRecovery: null
    function restoreUiState(data, restoreDraft = true) {
        if (restoreDraft)
            root.chat.restoreComposer(data);
        pendingRecovery = data;
        if (!data.threadId || (root.chat.threadId === data.threadId && root.chat.messages.count > 0))
            finishRecovery();
    }
    function finishRecovery() {
        const data = pendingRecovery;
        if (!data)
            return;
        pendingRecovery = null;
        if (data.threadId && root.chat.threadId && data.threadId !== root.chat.threadId)
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
        root.chat.start();
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
                enabled: !root.chat.busy
                onClicked: root.chat.newChat()
            }
            ActionButton {
                text: "Workspace"
                icon: "open_in_new"
                enabled: !root.chat.busy && root.chat.threadId.length > 0
                onClicked: root.chat.workspace()
            }
        }
        NacreText {
            width: parent.width
            elide: Text.ElideRight
            text: (root.chat.provider ? root.chat.provider.charAt(0).toUpperCase() + root.chat.provider.slice(1) : "Assistant") + " · " + root.chat.title
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
        model: root.chat.messages
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
                id: index >= 0 ? root.chat.messages.get(index).id : "",
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
                    const index = root.chat.itemIndex(savedPosition.id);
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
                    onClicked: root.chat.copy(bubble.text)
                }
                Repeater {
                    model: root.codeBlocks(bubble.text)
                    ActionButton {
                        required property string modelData
                        text: "Copy code"
                        onClicked: root.chat.copy(modelData)
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
        target: root.chat
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
            if (root.pendingRecovery && root.pendingRecovery.threadId === root.chat.threadId)
                return;
            transcript.savedPosition = {
                bottom: true
            };
            transcript.latest();
        }
    }
    DropArea {
        anchors.fill: parent
        enabled: root.chat.attachmentsSupported
        onDropped: drop => {
            if (drop.hasUrls) {
                root.chat.addFiles(drop.urls);
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
            visible: root.chat.error.length > 0 || root.chat.status.length > 0
            text: root.chat.error || root.chat.status
            color: root.chat.error ? NacreColours.palette.m3error : NacreColours.palette.m3onSurfaceVariant
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
            visible: root.chat.question !== null
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            Column {
                id: questionColumn
                width: questionScroll.availableWidth
                spacing: 10
                Repeater {
                    model: root.chat.question?.questions ?? []
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
                                    selected: root.chat.answers[q.modelData.id]?.answers?.[0] === text
                                    onClicked: root.chat.setAnswer(q.modelData.id, text)
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
                            onTextChanged: root.chat.setAnswer(q.modelData.id, text)
                        }
                    }
                }
                ActionButton {
                    objectName: "sidebarReply"
                    text: "Reply"
                    enabled: !root.chat.inWorkspace && root.chat.question !== null && root.chat.question.questions.every(q => (root.chat.answers[q.id]?.answers?.[0] ?? "").trim().length > 0)
                    onClicked: root.chat.answer()
                }
            }
        }
        Flow {
            width: parent.width
            spacing: 5
            Repeater {
                model: root.chat.attachments ?? []
                ActionButton {
                    required property var modelData
                    text: modelData.name
                    icon: "close"
                    onClicked: root.chat.removeAttachment(modelData.path)
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
                enabled: !root.chat.inWorkspace
                width: composer.availableWidth
                wrapMode: TextEdit.Wrap
                text: root.chat.draft
                onTextChanged: if (root.chat.draft !== text)
                    root.chat.draft = text
                placeholderText: root.chat.inWorkspace ? "Chat moved to the workspace" : root.chat.busy ? "Add a message while I work" : "Message Nacre AI"
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
            target: root.chat
            function onDraftChanged() {
                if (input.text !== root.chat.draft)
                    input.text = root.chat.draft;
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
                enabled: !root.chat.inWorkspace && (input.text.trim().length > 0 || (root.chat.attachments ?? []).length > 0)
                onClicked: root.send()
            }
            ActionButton {
                compact: true
                text: "Attach"
                icon: "attach_file"
                enabled: root.chat.attachmentsSupported
                onClicked: root.chat.pickFiles()
            }
            ActionButton {
                compact: true
                text: root.chat.busy ? "" : "Screenshot"
                icon: "screenshot"
                enabled: root.chat.attachmentsSupported
                onClicked: root.chat.screenshot()
            }
            ActionButton {
                compact: true
                text: ""
                icon: "stop"
                visible: root.chat.busy
                onClicked: root.chat.stop()
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
        if (!root.chat.inWorkspace && (input.text.trim() || (root.chat.attachments ?? []).length)) {
            transcript.latest();
            if (root.chat.send(input.text) === false)
                return;
            root.chat.draft = "";
            root.chat.attachments = [];
            input.text = "";
        }
    }
}
