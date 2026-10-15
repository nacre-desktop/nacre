import QtQuick
import "../../utils/scripts/window-title.js" as WindowTitle
import QtQuick.Controls.Basic
import qs.widgets
import qs.services
import qs.config

Item {
    id: root
    property string outputName: ""
    readonly property bool perOutput: NacreCompositor.capabilities?.perOutputWorkspaces === true
    readonly property var outputWorkspaces: perOutput ? NacreCompositor.workspaces.values.filter(value => value.outputId === outputName) : []
    // A complete configured set of named workspaces is persistent. Keep Niri's
    // automatic spare outside that set, including while another output has focus.
    readonly property var rowWorkspaces: {
        const named = outputWorkspaces.filter(value => !!value.name).sort((a, b) => a.index - b.index);
        return named.length >= NacreBar.workspaceCount ? named.slice(0, NacreBar.workspaceCount) : outputWorkspaces;
    }
    readonly property int activeWorkspaceIndex: perOutput ? outputWorkspaces.find(value => value.active)?.index ?? 0 : NacreCompositor.activeWsId
    property real availableWidth: 640
    property bool ready: false
    property Item activeCell: null
    property var displayIds: []
    property real wheelRemainder: 0
    readonly property bool showWindowHints: DesktopSettings.data.workspaceWindowHints !== false
    readonly property bool customIcons: NacreBar.workspaceIcons.some(icon => !!icon)
    readonly property var iconAllocation: {
        const result = {};
        for (const id of displayIds)
            result[id] = 0;
        if (!showWindowHints)
            return result;
        const active = activeWorkspaceIndex;
        const count = windowsForWorkspace(active).length;
        const base = displayIds.length * 32 + Math.max(0, displayIds.length - 1) * 2 + (count ? 4 : 0);
        let shown = Math.min(3, count, Math.max(0, Math.floor((availableWidth - base) / 18)));
        while (shown > 0 && base + shown * 18 + (shown < count ? 20 : 0) > availableWidth)
            shown--;
        result[active] = shown;
        return result;
    }
    readonly property int iconLimit: Math.max(0, ...Object.values(iconAllocation))
    readonly property bool motion: ready && visible && NacreTokens.motionEnabled
    onMotionChanged: {
        if (!motion && ready) {
            glide.complete();
            capsuleResize.complete();
            for (let index = 0; index < repeater.count; index++)
                repeater.itemAt(index)?.settleMotion();
        }
    }
    readonly property var workspaceIds: {
        if (perOutput)
            return rowWorkspaces.map(value => value.index).sort((a, b) => a - b);
        const ids = new Set(Array.from({
            length: NacreBar.workspaceCount
        }, (_, index) => index + 1));
        for (const id of [NacreCompositor.activeWsId, ...NacreCompositor.windows.map(client => client.workspace?.id)]) {
            if (Number.isInteger(id) && id >= 1)
                ids.add(id);
        }
        return [...ids].sort((a, b) => a - b);
    }
    implicitWidth: beads.implicitWidth
    implicitHeight: 30
    function synchronize() {
        activeCell = repeater.itemAt(displayIds.indexOf(activeWorkspaceIndex));
    }
    function synchronizeIds() {
        if (workspaceIds.length !== displayIds.length || workspaceIds.some((id, index) => id !== displayIds[index]))
            displayIds = workspaceIds;
        Qt.callLater(synchronize);
    }
    onWorkspaceIdsChanged: synchronizeIds()
    Component.onCompleted: {
        synchronizeIds();
        Qt.callLater(() => ready = true);
    }
    onActiveWorkspaceIndexChanged: Qt.callLater(synchronize)
    function workspace(index) {
        return outputWorkspaces.find(value => value.index === index);
    }
    function windowsForWorkspace(index) {
        if (!perOutput)
            return NacreCompositor.windows.filter(client => client.workspace?.id === index);
        const value = workspace(index);
        return value ? NacreCompositor.windows.filter(client => client.workspaceId === value.id) : [];
    }
    function activate(id) {
        if (!Number.isInteger(id) || !workspaceIds.includes(id))
            return false;
        // Display numbers repeat on different outputs and can be reindexed.
        const reference = perOutput ? workspace(id)?.id : id;
        return reference !== undefined ? NacreCompositor.switchWorkspace(reference) : false;
    }
    function step(direction) {
        if (!Number.isFinite(direction) || direction === 0)
            return false;
        const index = workspaceIds.indexOf(activeWorkspaceIndex);
        const next = index + Math.sign(direction);
        return index >= 0 && next >= 0 && next < workspaceIds.length ? activate(workspaceIds[next]) : false;
    }
    function scroll(delta) {
        if (!Number.isFinite(delta) || delta === 0)
            return false;
        if (wheelRemainder && Math.sign(wheelRemainder) !== Math.sign(delta))
            wheelRemainder = 0;
        wheelRemainder += delta;
        if (Math.abs(wheelRemainder) < 120)
            return true;
        const direction = wheelRemainder < 0 ? 1 : -1;
        wheelRemainder %= 120;
        step(direction);
        return true;
    }
    function tint(index, id) {
        const palette = NacreColours.palette;
        if (NacrePresentation.active.workspaceColors)
            return palette["m3" + ["primary", "secondary", "tertiary"][(id - 1) % 3]];
        const position = Math.min(1, index / Math.max(1, NacreBar.workspaceCount - 1));
        return position <= .5 ? Qt.tint(palette.m3primary, Qt.alpha(palette.m3secondary, position * 2)) : Qt.tint(palette.m3secondary, Qt.alpha(palette.m3tertiary, (position - .5) * 2));
    }
    Rectangle {
        objectName: "workspaceThread"
        z: -2
        x: 16
        y: 14.5
        width: Math.max(0, beads.width - 32)
        height: 1
        visible: !root.customIcons
        opacity: .12
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: NacreColours.palette.m3primary
            }
            GradientStop {
                position: .5
                color: NacreTokens.gradientColour(NacreColours.palette.m3primary, NacreColours.palette.m3secondary)
            }
            GradientStop {
                position: 1
                color: NacreTokens.gradientColour(NacreColours.palette.m3primary, NacreColours.palette.m3tertiary)
            }
        }
    }
    Row {
        id: beads
        spacing: 2
        Repeater {
            id: repeater
            model: root.displayIds
            onItemAdded: Qt.callLater(root.synchronize)
            onItemRemoved: Qt.callLater(root.synchronize)
            delegate: NacreSurface {
                id: cell
                required property int index
                required property int modelData
                readonly property int workspaceId: modelData
                readonly property bool selected: root.activeWorkspaceIndex === workspaceId
                readonly property var windows: root.windowsForWorkspace(workspaceId)
                readonly property int windowCount: windows.length
                readonly property bool occupied: windowCount > 0
                readonly property bool urgent: windows.some(client => client.urgent === true) || (root.perOutput && root.workspace(workspaceId)?.urgent === true)
                readonly property var shownWindows: windows.slice(0, root.iconAllocation[workspaceId] ?? 0)
                readonly property int overflow: selected && root.showWindowHints ? Math.max(0, windowCount - shownWindows.length) : 0
                readonly property color accent: root.tint(index, workspaceId)
                objectName: "nacreWorkspace" + workspaceId
                function settleMotion() {
                    cellResize.complete();
                }
                width: root.customIcons ? 46 : 32 + shownWindows.length * 18 + (overflow ? 20 : 0) + (selected && windowCount && root.showWindowHints ? 4 : 0)
                height: 30
                radius: 15
                color: root.customIcons && selected ? accent : pointer.hovered ? Qt.alpha(accent, .10) : "transparent"
                Behavior on width {
                    enabled: root.motion
                    NumberAnimation {
                        id: cellResize
                        duration: 160
                        easing.type: Easing.OutCubic
                    }
                }
                HoverHandler {
                    id: pointer
                }
                NacreWorkspacePearl {
                    objectName: "workspacePearl"
                    x: 16 - width / 2
                    y: (cell.height - height) / 2
                    visible: !root.customIcons && !cell.selected
                    accent: cell.accent
                    workspaceId: cell.workspaceId
                    occupied: cell.occupied
                    urgent: cell.urgent
                }
                Row {
                    anchors.centerIn: parent
                    visible: root.customIcons
                    spacing: 4
                    NacreIcon {
                        visible: !!text
                        text: NacreBar.workspaceIcons[cell.workspaceId - 1] || ""
                        font.pointSize: 11
                        color: cell.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurfaceVariant
                    }
                    NacreText {
                        text: cell.workspaceId
                        font.pointSize: 10
                        color: cell.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurfaceVariant
                    }
                }
                Rectangle {
                    visible: root.customIcons && cell.occupied
                    width: 3
                    height: 3
                    radius: 2
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 2
                    color: cell.selected ? NacreColours.palette.m3onPrimary : cell.accent
                }
                NacreInteraction {
                    accessibleName: NacreI18n.tr(cell.selected ? (cell.urgent ? "workspaces.description.active-urgent" : "workspaces.description.active") : (cell.urgent ? "workspaces.description.inactive-urgent" : "workspaces.description.inactive"), {
                        one: "Workspace {workspace}, {count} window",
                        other: "Workspace {workspace}, {count} windows"
                    }, {
                        workspace: cell.workspaceId,
                        count: cell.windowCount
                    })
                    function onClicked() {
                        root.activate(cell.workspaceId);
                    }
                }
                Row {
                    x: 33
                    objectName: "workspaceApps"
                    visible: !root.customIcons && cell.selected && root.showWindowHints
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 3
                    Repeater {
                        model: !root.customIcons ? cell.shownWindows : []
                        NacreWorkspaceAppIcon {
                            required property var modelData
                            client: modelData
                            color: cell.selected ? NacreColours.palette.m3onSurface : NacreColours.palette.m3onSurfaceVariant
                            NacreInteraction {
                                accessibleName: NacreI18n.tr("ui.workspace-row.accessibleName.focus-value0", "Focus {value0}", {
                                    value0: WindowTitle.caption(parent.client?.title || parent.client?.wmClass || "window", parent.client?.wmClass, ChatWindowTitle.titles[parent.client?.pid])
                                })
                                function onClicked() {
                                    NacreCompositor.focusWindow(parent.client?.windowId);
                                }
                            }
                        }
                    }
                    NacreText {
                        objectName: "workspaceOverflow"
                        visible: cell.overflow > 0
                        text: "+" + cell.overflow
                        font.pointSize: 9
                        color: NacreColours.palette.m3onSurface
                    }
                }
                ToolTip {
                    id: tip
                    visible: pointer.hovered
                    delay: 450
                    text: NacreI18n.tr(cell.selected ? "workspaces.tooltip.active" : "workspaces.tooltip.inactive", {
                        one: "Workspace {workspace} · {count} window{titles}{more}",
                        other: "Workspace {workspace} · {count} windows{titles}{more}"
                    }, {
                        workspace: cell.workspaceId,
                        count: cell.windowCount,
                        titles: cell.windowCount ? "\n" + cell.windows.slice(0, 5).map(client => NacreI18n.windowCaption(WindowTitle.caption(client.title || client.wmClass || NacreI18n.tr("workspaces.window.fallback", "Window"), client.wmClass, ChatWindowTitle.titles[client.pid]), client.wmClass).slice(0, 80)).join("\n") : "",
                        more: cell.windowCount > 5 ? NacreI18n.tr("workspaces.tooltip.more", "\n+{count} more", {
                            count: cell.windowCount - 5
                        }) : ""
                    })
                    contentItem: NacreText {
                        text: tip.text
                        font.pointSize: 10
                        textFormat: Text.PlainText
                    }
                    background: NacreSurface {
                        color: NacreColours.palette.m3surfaceContainerHigh
                        radius: 8
                        border.width: 1
                        border.color: NacreColours.palette.m3outlineVariant
                    }
                }
            }
        }
    }
    NacreSurface {
        id: current
        z: -1
        objectName: "activeWorkspaceCapsule"
        visible: !root.customIcons && !!root.activeCell
        x: root.activeCell?.x ?? 0
        y: 1
        width: root.activeCell?.width ?? 32
        height: 28
        radius: 14
        color: Qt.tint(NacreColours.palette.m3surfaceContainer, Qt.alpha(root.activeCell?.accent ?? NacreColours.palette.m3primary, .15))
        border.width: 1
        border.color: Qt.alpha(root.activeCell?.accent ?? NacreColours.palette.m3primary, .24)
        Behavior on x {
            enabled: root.motion
            NumberAnimation {
                id: glide
                duration: 180
                easing.type: Easing.OutCubic
            }
        }
        Behavior on width {
            enabled: root.motion
            NumberAnimation {
                id: capsuleResize
                duration: 160
                easing.type: Easing.OutCubic
            }
        }
        NacreWorkspacePearl {
            objectName: "activeWorkspacePearl"
            x: 5
            anchors.verticalCenter: parent.verticalCenter
            workspaceId: root.activeCell?.workspaceId ?? 0
            current: true
            occupied: true
            urgent: root.activeCell?.urgent ?? false
            accent: root.activeCell?.accent ?? NacreColours.palette.m3primary
        }
    }
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.NoButton
        onWheel: wheel => wheel.accepted = root.scroll(wheel.angleDelta.y || wheel.angleDelta.x)
    }
}
