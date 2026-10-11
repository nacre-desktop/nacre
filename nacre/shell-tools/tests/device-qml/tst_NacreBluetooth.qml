import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreBluetoothModels"
    when: windowShown
    Component {
        id: service
        NacreBluetooth {}
    }
    Component {
        id: adapter
        QtObject {
            property bool enabled: true
            property bool discovering: false
        }
    }
    Component {
        id: device
        QtObject {
            property string name: "Headphones"
            property string deviceName: "Original name"
            property string address: "00:11:22:33:44:55"
            property string icon: "audio-headphones"
            property bool connected: false
            property bool paired: true
            property bool trusted: true
        }
    }
    function test_native_alias_connection_removal_and_absent_adapter() {
        Bluetooth.defaultAdapter = null;
        Bluetooth.devices = {
            values: []
        };
        const state = createTemporaryObject(service, test);
        verify(!state.available);
        verify(!state.powered);
        compare(state.devices.length, 0);
        const radio = createTemporaryObject(adapter, test);
        const headphones = createTemporaryObject(device, test);
        Bluetooth.defaultAdapter = radio;
        Bluetooth.devices = {
            values: [headphones]
        };
        verify(state.powered);
        verify(!state.discovering);
        compare(state.devices.length, 1);
        verify(headphones.paired);
        verify(headphones.trusted);
        verify(!headphones.connected);
        verify(radio.enabled);
        verify(!radio.discovering);
        compare(state.devices[0].alias, "Headphones");
        compare(state.devices[0].name, "Original name");
        headphones.connected = true;
        verify(state.devices[0].connected);
        headphones.name = "Renamed";
        compare(state.devices[0].alias, "Renamed");
        radio.enabled = false;
        verify(!state.powered);
        compare(state.devices.length, 1);
        state.refresh();
        verify(headphones.connected);
        verify(!radio.enabled);
        verify(!radio.discovering);
        Bluetooth.devices = {
            values: []
        };
        compare(state.devices.length, 0);
        Bluetooth.defaultAdapter = null;
        verify(!state.available);
        verify(!state.discovering);
    }
}
