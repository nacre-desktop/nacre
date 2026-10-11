import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Shapes

Rectangle {
    id: root
    width: Screen.width
    height: Screen.height
    property var configuration: typeof config !== "undefined" ? config : ({})
    property var backend: typeof sddm !== "undefined" ? sddm : null
    property var usersModel: typeof userModel !== "undefined" ? userModel : []
    property var sessionsModel: typeof sessionModel !== "undefined" ? sessionModel : []
    property bool busy: false
    property string message: ""
    property date now: new Date()
    property alias password: password.text
    property alias selectedUser: users.currentIndex
    property alias selectedSession: sessions.currentIndex
    readonly property color primary: colour("primary", "#dbc492")
    readonly property color secondary: colour("secondary", "#d2c5ad")
    readonly property color foreground: colour("text", "#e7e1db")
    readonly property color muted: colour("muted", "#cec5b7")
    readonly property color raised: colour("raised", "#211f1c")
    readonly property color failure: colour("error", "#ffb4ab")
    function colour(key, fallback) {
        const value = String(configuration[key] ?? "");
        return /^#[0-9a-fA-F]{6}$/.test(value) ? value : fallback;
    }
    function submit() {
        if (busy || !password.text || !users.currentText || sessions.currentIndex < 0 || !backend)
            return;
        busy = true;
        message = "";
        backend.login(users.currentText, password.text, sessions.currentIndex);
    }
    function failed() {
        busy = false;
        password.clear();
        message = "Password not accepted. Try again.";
        password.forceActiveFocus();
    }
    function selectLast() {
        users.currentIndex = Math.max(0, usersModel.lastIndex ?? 0);
        sessions.currentIndex = Math.max(0, sessionsModel.lastIndex ?? 0);
        password.forceActiveFocus();
    }
    color: colour("surface", "#151310")
    Image {
        anchors.fill: parent
        source: root.configuration.background ?? ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }
    // Wallpaper is softly blurred when published, so the greeter needs no effects plugins.
    Rectangle {
        anchors.fill: parent
        color: root.color
        opacity: .23
    }
    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: root.now = new Date()
    }
    Connections {
        target: root.backend
        ignoreUnknownSignals: true
        function onLoginFailed() {
            root.failed();
        }
        function onLoginSucceeded() {
            password.clear();
        }
        function onInformationMessage(message) {
            root.busy = false;
            root.message = message;
        }
    }
    Logo {
        x: 36
        y: 30
        width: 48
        height: 48
        primary: root.primary
        secondary: root.secondary
        tertiary: root.colour("tertiary", "#8cc9cd")
        highlight: root.colour("highlight", "#eee9f4")
        background: root.color
        foreground: root.foreground
    }
    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height / 2 - 230
        width: 360
        spacing: 16
        Text {
            width: parent.width
            text: Qt.formatDateTime(root.now, "HH:mm")
            horizontalAlignment: Text.AlignHCenter
            font.family: "Cantarell"
            font.pointSize: 72
            color: root.foreground
        }
        Text {
            width: parent.width
            text: Qt.formatDateTime(root.now, "dddd, dd MMMM")
            horizontalAlignment: Text.AlignHCenter
            font.family: "Cantarell"
            font.pointSize: 16
            color: root.primary
        }
        Item {
            width: parent.width
            height: 34
        }
        Picker {
            id: users
            width: parent.width
            visible: count > 1
            model: root.usersModel
            textRole: "name"
            enabled: !root.busy
            contentItem: Text {
                text: users.displayText
                color: root.foreground
                font.family: "Cantarell"
                font.pointSize: 12
                verticalAlignment: Text.AlignVCenter
                leftPadding: 12
            }
            background: Rectangle {
                radius: 12
                color: root.raised
                border.color: root.primary
                border.width: users.activeFocus ? 2 : 1
            }
        }
        Text {
            width: parent.width
            visible: users.count === 1
            text: users.currentText
            color: root.muted
            font.family: "Cantarell"
            font.pointSize: 12
            horizontalAlignment: Text.AlignHCenter
        }
        TextField {
            id: password
            objectName: "passwordField"
            width: parent.width
            height: 54
            enabled: !root.busy
            echoMode: TextInput.Password
            placeholderText: "Enter your space"
            placeholderTextColor: root.muted
            color: root.foreground
            selectionColor: root.primary
            selectedTextColor: root.raised
            font.family: "Cantarell"
            font.pointSize: 13
            leftPadding: 16
            rightPadding: 16
            horizontalAlignment: TextInput.AlignHCenter
            selectByMouse: true
            background: Rectangle {
                color: root.raised
                radius: 12
                border.width: password.activeFocus ? 2 : 1
                border.color: root.message ? root.failure : root.primary
            }
            onAccepted: root.submit()
            Keys.onEscapePressed: clear()
        }
        Text {
            width: parent.width
            height: 24
            text: root.busy ? "Signing in…" : root.message
            wrapMode: Text.Wrap
            color: root.message ? root.failure : root.muted
            font.family: "Cantarell"
            font.pointSize: 11
            horizontalAlignment: Text.AlignHCenter
        }
        Text {
            width: parent.width
            visible: typeof keyboard !== "undefined" && keyboard.capsLock
            text: "Caps Lock is on"
            color: root.primary
            font.family: "Cantarell"
            font.pointSize: 11
            horizontalAlignment: Text.AlignHCenter
        }
    }
    Row {
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 30
        anchors.left: parent.left
        anchors.leftMargin: 36
        spacing: 12
        Picker {
            id: sessions
            width: 250
            height: 38
            model: root.sessionsModel
            textRole: "name"
            enabled: !root.busy
            contentItem: Text {
                text: sessions.displayText
                color: root.muted
                font.family: "Cantarell"
                font.pointSize: 11
                verticalAlignment: Text.AlignVCenter
                leftPadding: 12
            }
            background: Rectangle {
                color: root.raised
                radius: 12
                border.width: sessions.activeFocus ? 1 : 0
                border.color: root.primary
            }
        }
    }
    Row {
        anchors.right: parent.right
        anchors.rightMargin: 36
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 30
        spacing: 10
        Button {
            text: "Restart"
            visible: root.backend?.canReboot ?? false
            onClicked: root.backend.reboot()
            contentItem: Text {
                text: parent.text
                color: root.foreground
                font.family: "Cantarell"
                font.pointSize: 11
            }
            background: Rectangle {
                color: root.raised
                radius: 12
            }
        }
        Button {
            text: "Power off"
            visible: root.backend?.canPowerOff ?? false
            onClicked: root.backend.powerOff()
            contentItem: Text {
                text: parent.text
                color: root.failure
                font.family: "Cantarell"
                font.pointSize: 11
            }
            background: Rectangle {
                color: root.raised
                radius: 12
            }
        }
    }
    component Picker: ComboBox {
        palette.button: root.raised
        palette.buttonText: root.foreground
        palette.window: root.raised
        palette.windowText: root.foreground
        palette.base: root.raised
        palette.text: root.foreground
        palette.highlight: root.primary
        palette.highlightedText: root.raised
    }
    Component.onCompleted: Qt.callLater(selectLast)
}
