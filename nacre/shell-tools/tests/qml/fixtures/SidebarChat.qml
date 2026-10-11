pragma Singleton
import QtQuick

QtObject {
    property string defaultProvider: "codex"
    property bool busy: false
    property bool inWorkspace: false
    property string threadId: "fixture"
    property string provider: "codex"
    property string title: "Test"
    property string status: ""
    property string error: ""
    property var question: null
    property var answers: ({})
    property string draft: ""
    property var attachments: []
    property bool attachmentsSupported: false
    property var messages

    messages: ListModel {}

    property int replies: 0
    property int sends: 0
    property int stops: 0
    property string lastText: ""

    signal historyReplacing
    signal historyReplaced

    function setProvider(name) {
    }

    function itemIndex(id) {
        for (let i = 0; i < messages.count; i++)
            if (messages.get(i).id === id) {
                return i;
            }
        return -1;
    }

    function replaceHistory(rows) {
        historyReplacing();
        messages.clear();
        for (const row of rows)
            messages.append(row);
        historyReplaced();
    }

    function copy(text) {
    }

    function pickFiles() {
    }

    function screenshot() {
    }

    function removeAttachment(path) {
    }

    function addFiles(files) {
    }

    function start() {
    }

    function send(text) {
        sends++;
        lastText = text;
    }

    function stop() {
        stops++;
    }

    function newChat() {
    }

    function workspace() {
    }

    function answer() {
        replies++;
    }

    function setAnswer(id, text) {
        const next = Object.assign({}, answers);
        next[id] = {
            "answers": [text]
        };
        answers = next;
    }
}
