pragma Singleton
import QtQuick

QtObject {
    property int profile: PowerProfile.Balanced
    property bool hasPerformanceProfile: true
    property string degradationReason: ""
}
