import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreWifiSnapshots"
    when: windowShown
    Component {
        id: service
        NacreNetwork {}
    }
    function point(ssid, active, strength, suffix) {
        return {
            ssid: ssid,
            bssid: "00:11:22:33:44:" + suffix,
            strength: strength,
            frequency: 2437,
            active: active,
            interface: "wlan0"
        };
    }
    function test_atomic_publish_grouped_active_preference_and_failed_refresh() {
        const state = createTemporaryObject(service, test);
        const reader = findChild(state, "networkReader");
        verify(state.busy);
        compare(reader.starts, 1);
        const snapshot = {
            wifiEnabled: true,
            wifiInterface: "wlan0",
            networks: [point("Mesh", false, 90, "55"), point("Mesh", true, 30, "56"), point("Guest", false, 60, "57"), point("", false, 60, "58")]
        };
        verify(state.publish(JSON.stringify(snapshot)));
        compare(state.networks.length, 4);
        compare(state.visibleNetworks.length, 2);
        compare(state.active.ssid, "Mesh");
        verify(state.visibleNetworks[0].active);
        compare(state.visibleNetworks[0].strength, 30);
        const prior = state.snapshot;
        verify(!state.publish('malformed'));
        compare(state.snapshot, prior);
        verify(state.error.length > 0);
        verify(!state.publish(JSON.stringify({
            error: "NetworkManager unavailable"
        })));
        compare(state.snapshot, prior);
        verify(!state.publish(JSON.stringify({
            wifiEnabled: true,
            wifiInterface: "-bad",
            networks: []
        })));
        compare(state.snapshot, prior);
        verify(!state.publish(JSON.stringify({
            wifiEnabled: true,
            wifiInterface: "wlan0",
            networks: [point("Bad", true, 101, "55")]
        })));
        compare(state.snapshot, prior);
        reader.running = false;
        reader.exited(1, 0);
        verify(!state.busy);
        compare(state.snapshot, prior);
        verify(state.publish(JSON.stringify({
            wifiEnabled: false,
            wifiInterface: "",
            networks: []
        })));
        verify(!state.wifiEnabled);
        compare(state.visibleNetworks.length, 0);
        compare(state.error, "");
    }
    function test_event_bursts_and_busy_requests_coalesce_without_successful_polling() {
        const state = createTemporaryObject(service, test);
        const reader = findChild(state, "networkReader");
        for (let i = 0; i < 20; i++)
            state.scheduleRefresh();
        wait(260);
        compare(reader.starts, 1);
        verify(state.pending);
        state.refresh();
        state.refresh();
        compare(reader.starts, 1);
        reader.running = false;
        reader.exited(0, 0);
        wait(260);
        compare(reader.starts, 2);
        verify(!state.pending);
        reader.running = false;
        reader.exited(0, 0);
        wait(350);
        compare(reader.starts, 2);
    }
    function test_hidden_active_and_roaming_snapshot_replaces_connected_row() {
        const state = createTemporaryObject(service, test);
        state.publish(JSON.stringify({
            wifiEnabled: true,
            wifiInterface: "wlan0",
            networks: [point("", true, 40, "55"), point("Mesh", false, 70, "56")]
        }));
        compare(state.visibleNetworks.length, 2);
        verify(state.visibleNetworks[0].active);
        compare(state.active.ssid, "");
        state.publish(JSON.stringify({
            wifiEnabled: true,
            wifiInterface: "wlan0",
            networks: [point("Mesh", false, 90, "55"), point("Mesh", true, 20, "56")]
        }));
        compare(state.visibleNetworks.length, 1);
        compare(state.visibleNetworks[0].bssid, "00:11:22:33:44:56");
    }
}
