pragma Singleton
import QtQuick

QtObject {
    property int sequence: 0
    property var pending: ({})
    property bool ready: true
    property bool stopping: false
    readonly property int count: Object.keys(pending).length
    function submit(owner, input) {
        const id = ++sequence;
        pending = Object.assign({}, pending, {
            [id]: {
                owner: owner,
                input: input
            }
        });
        return id;
    }
    function cancel(id) {
        const next = Object.assign({}, pending);
        delete next[id];
        pending = next;
    }
    function reply(id, code, text) {
        const request = pending[id];
        cancel(id);
        request.owner.complete(code, 0, text);
    }
}
