pragma ComponentBehavior: Bound
import QtQuick
import qs.services
import qs.widgets
import qs.modules.launcher as Launcher

Column {
    id: root
    property int selected: 0
    readonly property var bundles: (library.data.themes || []).filter(item => item.bundle)
    spacing: 12
    NacreSoftwareLibrary {
        id: library
        active: root.visible
    }
    NacreSelect {
        width: parent.width
        model: root.bundles.map(item => item.name)
        currentIndex: root.selected
        Accessible.name: NacreI18n.tr("software.bundle.title", "Theme bundles")
        onActivated: index => root.selected = index
    }
    Launcher.NacreBundlePreview {
        width: parent.width
        visible: root.bundles.length > 0
        entry: Object.assign({}, root.bundles[root.selected] || {}, {
            themeModes: root.bundles[root.selected]?.modes || {}
        })
        busy: library.busy || NacreWallpapers.themeBusy
        onApplyRequested: (mode, scope) => library.work("bundle-apply", root.bundles[root.selected].id, {
                mode: mode,
                scope: scope
            })
    }
    NacreText {
        width: parent.width
        visible: !!library.message
        text: library.message
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }
}
