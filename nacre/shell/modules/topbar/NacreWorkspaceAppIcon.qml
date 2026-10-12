import QtQuick
import qs.widgets
import qs.services
import qs.utils

Item {
    id: root
    required property var client
    property int size: 15
    property color color: NacreColours.palette.m3onSurfaceVariant
    implicitWidth: size
    implicitHeight: size
    readonly property string symbol: {
        const name = String(client?.wmClass || "");
        const klass = name.toLowerCase();
        const mark = /^nacre-(ai|brain|colors|settings|welcome|control)(?:-|$)/.exec(klass);
        const nacre = {
            ai: "neurology",
            brain: "psychology",
            colors: "palette",
            settings: "tune",
            welcome: "waving_hand",
            control: "tune"
        };
        if (mark)
            return nacre[mark[1]];
        const categories = NacreIcons.getDesktopEntry(name)?.categories || [];
        const specific = [["TerminalEmulator", "terminal"], ["InstantMessaging", "forum"], ["Email", "mail"], ["WebBrowser", "language"], ["Audio", "music_note"], ["Video", "movie"], ["Development", "code"], ["Game", "sports_esports"], ["Graphics", "palette"], ["Office", "description"], ["Settings", "tune"]];
        for (const [category, glyph] of specific)
            if (categories.includes(category))
                return glyph;
        return NacreIcons.getAppCategoryIcon(name, "apps");
    }
    NacreIcon {
        objectName: "workspaceCategoryGlyph"
        anchors.fill: parent
        text: root.symbol
        color: root.color
        font.pointSize: root.size * .7
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
    Accessible.ignored: true
}
