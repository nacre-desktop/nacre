import QtQuick
import Quickshell
import qs.widgets

ShellRoot {
    FloatingWindow {
        visible: true
        implicitWidth: 300
        implicitHeight: 180
        Item {
            id: sheet
            anchors.fill: parent
            Rectangle {
                anchors.fill: parent
                color: "#f0f0f4"
            }
            NacreClip {
                id: clipper
                x: 20
                y: 20
                width: 100
                height: 100
                radius: 50
                color: "#118844"
                Rectangle {
                    anchors.fill: parent
                    color: "#2679cd"
                }
            }
            NacreSurface {
                x: 150
                y: 20
                width: 120
                height: 60
                radius: 20
                color: "#303640"
                NacreText {
                    anchors.centerIn: parent
                    text: "Nacre"
                    color: "#fafafa"
                }
            }
            Timer {
                interval: 500
                running: true
                onTriggered: sheet.grabToImage(result => {
                    if (!result.saveToFile(Qt.resolvedUrl("capture.png").toString().slice(7)))
                        throw new Error("Could not save native capture");
                    console.log("FOUNDATION_CAPTURED");
                    Qt.quit();
                })
            }
        }
    }
}
