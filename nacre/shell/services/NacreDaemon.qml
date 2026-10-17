pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils
import "daemon-state.js" as State

Singleton {
    id: root
    // Foundation remains opt-in until live daemon activation is approved.
    // Set to 1 in private/nested sessions; 0 keeps the one-release legacy path.
    property bool enabled: Quickshell.env("NACRE_DAEMON_CLIENT") === "1"
    property bool connected: false
    property bool stale: false
    property string error: ""
    property string boot: ""
    property var methods: ({})
    property var advertisedTopics: ({})
    property var topics: Object.create(null)
    property var pending: Object.create(null)
    property double sequence: 0
    property int retryMs: 50
    property var uiOwner: null
    property var uiHandler: null
    property int pendingCount: 0
    property var ipcTargets: Object.create(null)
    signal event(string topic, string name, var data, double sequence)
    signal lost(string topic)
    signal newBoot(string id)

    function registerIpc(name, handler, signatures) {
        if (ipcTargets[name] && ipcTargets[name].handler !== handler)
            return false;
        if (!ipcTargets[name] && Object.keys(ipcTargets).length >= 64)
            return false;
        ipcTargets[name] = {
            handler: handler,
            signatures: signatures
        };
        return true;
    }
    function unregisterIpc(name, handler) {
        if (ipcTargets[name]?.handler === handler)
            delete ipcTargets[name];
    }
    function dispatchIpc(params) {
        const refuse = (code, message) => ({
                    err: {
                        code: code,
                        message: message,
                        retry: false
                    }
                });
        if (!params || typeof params.target !== "string" || typeof params.method !== "string" || !Array.isArray(params.args) || params.args.some(value => typeof value !== "string"))
            return refuse("invalid_request", "Invalid UI arguments");
        const target = ipcTargets[params.target];
        if (!target || !Object.prototype.hasOwnProperty.call(target.signatures, params.method))
            return refuse("unknown_method", "UI method is unavailable");
        const signature = target.signatures[params.method];
        if (signature.length !== params.args.length)
            return refuse("validation", "Invalid UI argument count");
        const args = [];
        for (let index = 0; index < signature.length; index++) {
            const value = params.args[index];
            if (signature[index] === "s") {
                args.push(value);
            } else {
                if (!value.trim())
                    return refuse("validation", "Invalid UI number");
                const number = Number(value);
                if (!Number.isFinite(number) || signature[index] === "i" && (!Number.isInteger(number) || number < -2147483648 || number > 2147483647))
                    return refuse("validation", "Invalid UI number");
                args.push(number);
            }
        }
        try {
            const value = target.handler[params.method].apply(target.handler, args);
            return {
                ok: value === undefined ? null : value
            };
        } catch (failure) {
            return refuse("internal", "UI action failed");
        }
    }

    function topic(name) {
        if (!Object.prototype.hasOwnProperty.call(topics, name)) {
            if (Object.keys(topics).length >= 128)
                throw new Error("Daemon topic limit reached");
            const value = topicFactory.createObject(root, {
                name: name
            });
            if (!value)
                throw new Error("Cannot create daemon topic");
            topics[name] = value;
        }
        return topics[name];
    }
    function subscribe(name) {
        const value = topic(name);
        value.users++;
        if (connected && value.users === 1 && advertisedTopics[name])
            send({
                t: "sub",
                topics: [name]
            });
        return value;
    }
    function unsubscribe(name) {
        const value = topics[name];
        if (!value || value.users <= 0)
            return;
        value.users--;
        if (connected && value.users === 0)
            send({
                t: "unsub",
                topics: [name]
            });
    }
    function resync(value) {
        value.fresh = false;
        if (!value.resyncing && connected) {
            value.resyncing = true;
            send({
                t: "resync",
                topics: [value.name]
            });
        }
    }
    function send(value) {
        if (!bridge.running)
            return false;
        try {
            bridge.write(JSON.stringify(value) + "\n");
            return true;
        } catch (failure) {
            error = "Daemon connection write failed";
            bridge.running = false;
            return false;
        }
    }
    function complete(callback, value, failure) {
        if (callback)
            Qt.callLater(() => {
                try {
                    callback(value, failure);
                } catch (error) {
                    console.warn("NacreDaemon: call callback failed");
                }
            });
    }
    function reject(callback, code, message, dispatched = undefined) {
        const failure = {
            code: code,
            message: message,
            retry: ["unavailable", "busy", "timeout"].includes(code)
        };
        if (dispatched !== undefined)
            failure.dispatched = dispatched;
        complete(callback, null, failure);
        return 0;
    }
    function call(method, params, callback, timeout = 0) {
        if (!enabled || !connected)
            return reject(callback, "unavailable", "Daemon is disconnected", false);
        const descriptor = Object.prototype.hasOwnProperty.call(methods, method) ? methods[method] : null;
        if (!descriptor)
            return reject(callback, "unknown_method", "Daemon method is unavailable", false);
        if (pendingCount >= 64)
            return reject(callback, "busy", "Daemon call queue is full", false);
        const milliseconds = timeout || descriptor.timeout;
        if (!Number.isInteger(milliseconds) || milliseconds < 1 || milliseconds > 120000)
            return reject(callback, "invalid_request", "Invalid daemon call deadline", false);
        if (sequence >= 9007199254740991) {
            if (pendingCount)
                return reject(callback, "busy", "Daemon call IDs are exhausted", false);
            sequence = 0;
        }
        const id = ++sequence;
        pending[id] = {
            callback: callback,
            deadline: Date.now() + milliseconds
        };
        // A write attempt can have reached the daemon even if its pipe fails.
        // Never retry a pending action after losing its reply.
        send({
            t: "call",
            id: id,
            m: method,
            p: params === undefined ? {} : params,
            timeout: milliseconds
        });
        deadlines();
        return id;
    }
    function cancel(id) {
        delete pending[id];
        deadlines();
    }
    function deadlines() {
        deadline.stop();
        const now = Date.now();
        let next = Infinity;
        for (const id of Object.keys(pending)) {
            const record = pending[id];
            if (record.deadline <= now) {
                delete pending[id];
                complete(record.callback, null, {
                    code: "timeout",
                    message: "Daemon call timed out; the action may already have completed",
                    retry: true
                });
            } else
                next = Math.min(next, record.deadline);
        }
        pendingCount = Object.keys(pending).length;
        if (Number.isFinite(next)) {
            deadline.interval = Math.max(1, next - now);
            deadline.start();
        }
    }
    function registerUi(owner, handler) {
        if (uiOwner && uiOwner !== owner)
            return false;
        uiOwner = owner;
        uiHandler = handler;
        if (connected)
            registerProvider();
        return true;
    }
    function unregisterUi(owner) {
        if (uiOwner !== owner)
            return;
        uiOwner = null;
        uiHandler = null;
        // Registration belongs to the connection; reconnect without it.
        bridge.running = false;
    }
    function registerProvider() {
        if (uiHandler)
            send({
                t: "register",
                provider: "ui",
                methods: {
                    "ui.dispatch": 1,
                    "ui.menu.activate": 1
                }
            });
    }
    function receive(text) {
        try {
            const message = JSON.parse(text);
            if (message.t === "welcome") {
                if (message.proto !== 1 || message.min > 1)
                    throw new Error();
                const changed = boot !== message.boot;
                boot = message.boot;
                methods = message.methods;
                advertisedTopics = message.topics;
                connected = true;
                stale = false;
                error = "";
                staleDelay.stop();
                retryMs = 50;
                const wanted = [];
                for (const value of Object.values(topics)) {
                    value.fresh = false;
                    value.resyncing = false;
                    if (value.users && advertisedTopics[value.name])
                        wanted.push(value.name);
                }
                if (wanted.length)
                    send({
                        t: "sub",
                        topics: wanted
                    });
                registerProvider();
                if (changed)
                    newBoot(boot);
            } else if (message.t === "snap" || message.t === "patch") {
                const value = topics[message.topic];
                if (!value || !value.users)
                    return;
                if (message.t === "snap") {
                    value.data = message.data;
                    value.sequence = message.seq;
                    value.resyncing = false;
                    value.fresh = true;
                } else if (value.fresh && message.seq <= value.sequence) {
                    return;
                } else if (!value.fresh || message.base !== value.sequence) {
                    resync(value);
                } else {
                    value.data = State.merge(value.data, message.data);
                    value.sequence = message.seq;
                }
            } else if (message.t === "ret") {
                const record = pending[message.id];
                if (record) {
                    delete pending[message.id];
                    deadlines();
                    complete(record.callback, message.ok, message.err);
                }
            } else if (message.t === "ev") {
                event(message.topic, message.name, message.data, message.seq);
            } else if (message.t === "lost") {
                if (topics[message.topic])
                    resync(topics[message.topic]);
                lost(message.topic);
            } else if (message.t === "call") {
                // This direction is independent of pending outbound activations.
                let response;
                try {
                    response = uiHandler ? uiHandler(message.m, message.p) : {
                        err: {
                            code: "unavailable",
                            message: "UI provider is unavailable",
                            retry: true
                        }
                    };
                } catch (failure) {
                    response = {
                        err: {
                            code: "internal",
                            message: "UI provider failed",
                            retry: false
                        }
                    };
                }
                send(Object.assign({
                    t: "ret",
                    id: message.id
                }, response));
            } else if (message.t === "ping") {
                send({
                    t: "pong",
                    id: message.id
                });
            } else if (message.t === "error") {
                error = message.err.message;
                if (!connected || !message.err.retry)
                    bridge.running = false;
            }
        } catch (failure) {
            error = "Invalid daemon response";
            bridge.running = false;
        }
    }
    function disconnected() {
        connected = false;
        methods = ({});
        advertisedTopics = ({});
        deadline.stop();
        const records = Object.values(pending);
        pending = Object.create(null);
        pendingCount = 0;
        for (const record of records)
            complete(record.callback, null, {
                code: "unavailable",
                message: "Daemon disconnected; the action may already have completed",
                retry: true
            });
        for (const value of Object.values(topics)) {
            value.fresh = false;
            value.resyncing = false;
        }
        if (enabled) {
            if (!stale && !staleDelay.running)
                staleDelay.start();
            retry.interval = retryMs;
            retryMs = Math.min(2000, retryMs * 2);
            retry.restart();
        }
    }
    onEnabledChanged: {
        retry.stop();
        if (enabled) {
            staleDelay.restart();
            bridge.running = true;
        } else {
            staleDelay.stop();
            stale = false;
            bridge.running = false;
        }
    }
    Component.onCompleted: if (enabled) {
        staleDelay.start();
        bridge.running = true;
    }
    Component {
        id: topicFactory
        QtObject {
            property string name: ""
            property var data: ({})
            property double sequence: 0
            property int users: 0
            property bool fresh: false
            property bool resyncing: false
        }
    }
    Process {
        id: bridge
        objectName: "nacreDaemonLink"
        command: [NacrePaths.helperBinary, "ctl", "attach"]
        stdinEnabled: true
        stdout: SplitParser {
            onRead: text => root.receive(text)
        }
        stderr: StdioCollector {}
        onStarted: root.send({
            t: "hello",
            proto: 1,
            min: 1,
            client: "qml"
        })
        onExited: {
            if (!root.error)
                root.error = stderr.text.trim() || "Daemon is disconnected";
            root.disconnected();
        }
    }
    Timer {
        id: retry
        onTriggered: if (root.enabled)
            bridge.running = true
    }
    Timer {
        id: deadline
        onTriggered: root.deadlines()
    }
    Timer {
        id: staleDelay
        interval: 2000
        onTriggered: root.stale = !root.connected
    }
}
