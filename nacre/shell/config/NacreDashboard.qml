pragma Singleton
import QtQuick

QtObject {
    property int mediaUpdateInterval: 500
    property int visualiserBars: 45
    property QtObject sizes: QtObject {
        property int tabIndicatorHeight: 3
        property int tabIndicatorSpacing: 5
        property int infoWidth: 200
        property int infoIconSize: 25
        property int dateTimeWidth: 110
        property int mediaWidth: 200
        property int mediaProgressSweep: 180
        property int mediaProgressThickness: 8
        property int resourceProgessThickness: 10
        property int weatherWidth: 250
        property int mediaCoverArtSize: 150
        property int mediaVisualiserSize: 80
        property int resourceSize: 200
    }
}
