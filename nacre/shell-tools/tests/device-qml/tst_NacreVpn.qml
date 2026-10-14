import QtQuick
import QtTest
import "fixtures"

TestCase {
    name: "SavedVpnControls"
    when: windowShown
    function test_readonly_snapshot_escaping_and_guarded_actions() {
        const reader = findChild(NacreVpn, "controlVpnReader"), worker = findChild(NacreVpn, "controlVpnWorker");
        compare(reader.starts, 0);
        compare(worker.starts, 0);
        NacreVpn.receive("11111111-1111-4111-8111-111111111111:vpn:Nacre\\: Studio:--\n22222222-2222-4222-8222-222222222222:wireguard:Travel:tun0\n33333333-3333-4333-8333-333333333333:ethernet:Wired:eth0");
        compare(NacreVpn.profiles.length, 2);
        compare(NacreVpn.profiles[0].name, "Nacre: Studio");
        compare(NacreVpn.connected[0].name, "Travel");
        compare(worker.starts, 0);
        verify(!NacreVpn.activate("missing", true));
        verify(!NacreVpn.activate(NacreVpn.profiles[0].uuid, "yes"));
        verify(NacreVpn.activate(NacreVpn.profiles[0].uuid, true));
        compare(worker.command.slice(-3).join("|"), "up|uuid|11111111-1111-4111-8111-111111111111");
        verify(!NacreVpn.activate(NacreVpn.profiles[0].uuid, false));
        worker.running = false;
        worker.exited(1, 0);
        verify(NacreVpn.error.length > 0);
        verify(reader.running);
    }
}
