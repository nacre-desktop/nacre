import QtQuick
import "../pages" as Pages

Loader {
    required property string target
    sourceComponent: ({
            displays: displays,
            sound: sound,
            network: network,
            bluetooth: bluetooth,
            time: time,
            workflows: workflows,
            notifications: notifications,
            system: system,
            ai: ai
        })[target]
    Component {
        id: displays
        Pages.NacreDisplaysPage {}
    }
    Component {
        id: sound
        Pages.NacreSoundPage {}
    }
    Component {
        id: network
        Pages.NacreNetworkPage {}
    }
    Component {
        id: bluetooth
        Pages.NacreBluetoothPage {}
    }
    Component {
        id: time
        Pages.NacreTimePage {}
    }
    Component {
        id: workflows
        Pages.NacreWorkflowsPage {}
    }
    Component {
        id: notifications
        Pages.NacreNotificationPage {}
    }
    Component {
        id: system
        Pages.NacreMaintenancePage {}
    }
    Component {
        id: ai
        Pages.NacreAiPage {}
    }
}
