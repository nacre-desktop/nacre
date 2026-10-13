import QtQuick
import Quickshell.Io
import qs.widgets
import qs.services

Item {
    id: root
    property bool active: false
    property var gpu: null
    property real used: NaN
    property real total: NaN
    readonly property string card: /^card[0-9]+$/.test(gpu?.card || "") ? gpu.card : ""
    readonly property string display: Number.isFinite(total) && total > 0 ? NacreI18n.tr("dashboard.gpu-memory.vram", "{used}/{total} GiB VRAM", {
        used: (used / 1073741824).toFixed(1),
        total: (total / 1073741824).toFixed(1)
    }) : gpu?.vendor === "Intel" ? NacreI18n.tr("dashboard.gpu-memory.shared", "Shared memory") : NacreI18n.tr("dashboard.gpu-memory.unavailable", "VRAM unavailable")
    function refresh() {
        if (!active)
            return;
        if (gpu?.vendor === "NVIDIA")
            nvidia.running = true;
        else if (card) {
            usedFile.reload();
            totalFile.reload();
        }
    }
    onActiveChanged: if (active)
        refresh()
    else
        nvidia.running = false
    onGpuChanged: {
        used = NaN;
        total = NaN;
        refresh();
    }
    onCardChanged: refresh()
    Component.onCompleted: refresh()
    Timer {
        interval: 10000
        repeat: true
        running: root.active
        onTriggered: root.refresh()
    }
    FileView {
        id: usedFile
        path: root.active && root.card ? "/sys/class/drm/" + root.card + "/device/mem_info_vram_used" : ""
        printErrors: false
        onLoaded: root.used = Number(text().trim())
    }
    FileView {
        id: totalFile
        path: root.active && root.card ? "/sys/class/drm/" + root.card + "/device/mem_info_vram_total" : ""
        printErrors: false
        onLoaded: root.total = Number(text().trim())
    }
    Process {
        id: nvidia
        command: ["nvidia-smi", "--query-gpu=memory.used,memory.total", "--format=csv,noheader,nounits", "--id=" + (root.gpu?.id || "0")]
        stdout: StdioCollector {
            onStreamFinished: {
                const values = text.split("\n")[0].split(",").map(Number);
                if (values.length === 2 && values.every(Number.isFinite)) {
                    root.used = values[0] * 1048576;
                    root.total = values[1] * 1048576;
                }
            }
        }
    }
}
