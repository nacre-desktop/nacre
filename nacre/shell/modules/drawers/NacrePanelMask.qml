import QtQuick
import Quickshell

Region {
    required property var controller
    Box {
        bounds: controller.modal ? Qt.rect(0, 0, controller.width, controller.height) : Qt.rect(0, 0, 0, 0)
    }
    Box {
        bounds: controller.headerRect
    }
    Box {
        bounds: controller.expand(controller.launcherRect)
    }
    Box {
        bounds: controller.expand(controller.dashboardRect)
    }
    Box {
        bounds: controller.expand(controller.leftRect)
    }
    Box {
        bounds: controller.expand(controller.osdRect)
    }
    Box {
        bounds: controller.expand(controller.sessionRect)
    }
    Box {
        bounds: controller.expand(controller.popoutRect)
    }
    Box {
        bounds: controller.notificationRect
    }
    Box {
        bounds: controller.leftEdgeAvailable && controller.leftEdgeRect.width ? controller.leftEdgeRect : Qt.rect(0, 0, 0, 0)
    }
    Box {
        bounds: controller.rightEdgeAvailable && controller.rightEdgeRect.width ? controller.rightEdgeRect : Qt.rect(0, 0, 0, 0)
    }

    Box {
        bounds: controller.topEdgeAvailable ? controller.topEdgeRect : Qt.rect(0, 0, 0, 0)
    }

    component Box: Region {
        property rect bounds
        x: Math.floor(bounds.x)
        y: Math.floor(bounds.y)
        width: Math.max(0, Math.ceil(bounds.x + bounds.width) - x)
        height: Math.max(0, Math.ceil(bounds.y + bounds.height) - y)
    }
}
