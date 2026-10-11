import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreResourceCounters"
    when: windowShown
    Component {
        id: service
        NacreSystemUsage {}
    }
    Component {
        id: visible
        QtObject {
            property bool dashboard: false
            property int dashboardTab: 0
        }
    }
    function test_counter_baseline_guest_reset_and_memory_fallback() {
        const state = createTemporaryObject(service, test);
        state.cpuSample("cpu 100 0 50 800 50 0 0 0 70 20\n");
        verify(isNaN(state.cpuPerc));
        state.cpuSample("cpu 140 0 60 840 60 0 0 0 90 30\n");
        compare(state.cpuPerc, .5);
        state.cpuSample("cpu 1 0 1 8 0 0 0 0\n");
        verify(isNaN(state.cpuPerc));
        state.memorySample("MemTotal: 1000 kB\nMemAvailable: 250 kB\n");
        compare(state.memUsed, 750);
        compare(state.memPerc, .75);
        state.memorySample("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 200 kB\nSReclaimable: 50 kB\nShmem: 20 kB\n");
        compare(state.memUsed, 620);
        state.memorySample("garbage");
        compare(state.memUsed, 620);
        state.slowSample(JSON.stringify({
            cpuTemp: null,
            gpuTemp: null,
            gpuPerc: null,
            gpuUsageAvailable: false,
            storageUsed: 3000000000,
            storageTotal: 4000000000
        }));
        verify(isNaN(state.cpuTemp));
        verify(isNaN(state.gpuPerc));
        verify(!state.gpuUsageAvailable);
        compare(state.storageUsed, 3000000000);
        compare(state.storagePerc, .75);
        compare(state.formatKib(1048576).join(" "), "1.0 GiB");
    }
    function test_no_fast_or_slow_timer_hidden_and_rebase_when_reopened() {
        const view = createTemporaryObject(visible, test);
        NacrePanelState.screens = {
            fixture: view
        };
        const state = createTemporaryObject(service, test);
        verify(!state.visibleDashboard);
        verify(!findChild(state, "resourceFastTimer").running);
        verify(!findChild(state, "resourceSlowTimer").running);
        const reads = state.fastReads;
        wait(1100);
        compare(state.fastReads, reads);
        state.previousCpu = {
            total: 100,
            idle: 80
        };
        view.dashboard = true;
        verify(state.visibleDashboard);
        compare(state.previousCpu, null);
        verify(findChild(state, "resourceFastTimer").running);
        view.dashboardTab = 1;
        verify(!state.visibleDashboard);
        verify(!findChild(state, "resourceFastTimer").running);
        view.dashboardTab = 3;
        verify(state.visibleDashboard);
        view.dashboard = false;
        NacrePanelState.screens = {};
        verify(!state.visibleDashboard);
    }
}
