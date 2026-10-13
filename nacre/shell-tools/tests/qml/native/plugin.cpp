#include <QObject>
#include <QQmlEngine>
#include <QQmlExtensionPlugin>

class NotificationStub : public QObject {
    Q_OBJECT
    Q_PROPERTY(int id MEMBER identifier)
    Q_PROPERTY(int noticeId MEMBER identifier)
    Q_PROPERTY(bool transient MEMBER temporary NOTIFY changed)
    Q_PROPERTY(bool temporary MEMBER temporary NOTIFY changed)
public:
    using QObject::QObject;
    int identifier = 1;
    bool temporary = false;
signals:
    void changed();
};

class LayerProperties : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString namespace MEMBER name)
    Q_PROPERTY(int layer MEMBER layer)
    Q_PROPERTY(int exclusionMode MEMBER exclusionMode)
    Q_PROPERTY(int keyboardFocus MEMBER keyboardFocus)
public:
    using QObject::QObject;
    QString name;
    int layer = 0, exclusionMode = 0, keyboardFocus = 0;
};

class WlrLayershell : public QObject {
    Q_OBJECT
public:
    static LayerProperties *qmlAttachedProperties(QObject *owner) {
        return new LayerProperties(owner);
    }
};
QML_DECLARE_TYPEINFO(WlrLayershell, QML_HAS_ATTACHED_PROPERTIES)

class StubPlugin : public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QQmlExtensionInterface_iid)
public:
    void registerTypes(const char *uri) override {
        if (QString(uri).endsWith("Notifications"))
            qmlRegisterType<NotificationStub>(uri, 1, 0, "NotificationStub");
        else
            qmlRegisterUncreatableType<WlrLayershell>(uri, 1, 0, "WlrLayershell", "Attached properties only");
    }
};
#include "plugin.moc"
