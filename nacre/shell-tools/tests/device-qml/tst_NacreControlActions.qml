import QtQuick
import QtTest
import "fixtures"

TestCase {
    name: "InlineWifiConnection"
    when: windowShown
    function test_inline_wifi_credentials_use_stdin_and_reject_stale_target() {
        const connector = findChild(NacreControlActions, "inlineWifiConnector");
        const point = NacreNetwork.visibleNetworks[0];
        verify(!NacreControlActions.connectWifiInline({
            ssid: "Missing",
            bssid: "00:00:00:00:00:00"
        }, "secret"));
        verify(!NacreControlActions.connectWifiInline(point, "bad\npassword"));
        verify(NacreControlActions.connectWifiInline(point, "fixture-secret"));
        verify(NacreControlActions.busy);
        verify(!connector.command.includes("fixture-secret"));
        wait(0);
        compare(connector.writes[0], "fixture-secret\n");
        compare(NacreControlActions.wifiPassword, "");
        verify(!NacreControlActions.connectWifiInline(point, "another"));
        connector.running = false;
        connector.exited(0, 0);
        compare(NacreControlActions.message, "");
        verify(NacreControlActions.connectWifiInline(point, "fixture-secret"));
        NacreControlActions.cancelWifiInline();
        compare(connector.signals[0], 15);
        connector.running = false;
        connector.exited(15, 0);
        compare(NacreControlActions.message, "");
        compare(NacreControlActions.wifiPassword, "");
    }
}
