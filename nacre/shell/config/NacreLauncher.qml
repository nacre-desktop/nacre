pragma Singleton
import QtQuick

QtObject {
    property int maxShown: 8
    property int maxWallpapers: 9
    property string actionPrefix: ">"
    property QtObject sizes: QtObject {
        property int itemWidth: 600
        property int itemHeight: 57
        property int wallpaperWidth: 280
        property int wallpaperHeight: 200
    }
}
