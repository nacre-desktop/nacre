import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "WifiConnectionLifecycle"
    when: windowShown
    Component {
        id: service
        DeviceActions {}
    }
    function test_connection_window_blocks_duplicate_actions_and_refreshes_on_exit() {
        const actions = createTemporaryObject(service, test);
        const connector = findChild(actions, "wifiConnector");
        const before = NacreNetwork.refreshes;
        actions.connectWifi("Fixture Guest");
        verify(actions.busy);
        compare(connector.starts, 1);
        compare(connector.command.slice(-2).join("|"), "wifi-connect|Fixture Guest");
        verify(connector.command.includes("-u"));
        verify(actions.message.length > 0);
        actions.connectWifi("Other");
        actions.request(["wifi-radio", "off"]);
        compare(connector.starts, 1);
        compare(actions.lastAction, "wifi-connect");
        connector.running = false;
        connector.exited(0, 0);
        verify(!actions.busy);
        compare(actions.message, "");
        compare(NacreNetwork.refreshes, before + 1);
        actions.connectWifi("Other");
        connector.running = false;
        connector.exited(1, 0);
        verify(!actions.busy);
        verify(actions.message.length > 0);
        compare(NacreNetwork.refreshes, before + 2);
    }
}
