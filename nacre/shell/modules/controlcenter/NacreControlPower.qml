import QtQuick
import qs.services
import qs.modules.bar.popouts as Details

Column {
    spacing: 24
    Details.NacreBatteryPopup {
        width: parent.width
        showSummary: false
        profilesEnabled: NacreControlTools.data.powerProfilesSupported === true
    }
    NacreControlDisplays {
        width: parent.width
    }
}
