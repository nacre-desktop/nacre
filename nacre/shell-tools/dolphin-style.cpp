// App-scoped native Breeze style with live colors from Nacre's KDE publisher.
#include <KColorScheme>
#include <KConfigGroup>
#include <KSharedConfig>
#include <QApplication>
#include <QDir>
#include <QEvent>
#include <QFile>
#include <QFileSystemWatcher>
#include <QFont>
#include <QIcon>
#include <QPointer>
#include <QProxyStyle>
#include <QSettings>
#include <QStyleFactory>
#include <QStylePlugin>
#include <QTimer>
#include <QWidget>

static QStyle *nativeStyle() {
    auto style = QStyleFactory::create("Breeze");
    return style ? style : QStyleFactory::create("Fusion");
}

class NacreDolphinStyle : public QProxyStyle {
  public:
    NacreDolphinStyle() : QProxyStyle(nativeStyle()) {
        timer.setSingleShot(true);
        timer.setInterval(40);
        QObject::connect(&timer, &QTimer::timeout, this, [this] { refresh(); });
        QObject::connect(&watcher, &QFileSystemWatcher::directoryChanged, this,
                         [this] { timer.start(); });
        QTimer::singleShot(0, this, [this] {
            if (QCoreApplication::applicationName().compare(
                    "dolphin", Qt::CaseInsensitive) != 0)
                return;
            const auto home = QDir::homePath();
            source = home + "/.local/share/color-schemes/Nacre.colors";
            watcher.addPaths(
                {home + "/.local/share/color-schemes", home + "/.config"});
            refresh();
        });
    }
    int styleHint(StyleHint hint, const QStyleOption *option,
                  const QWidget *widget,
                  QStyleHintReturn *data) const override {
        if (hint == SH_ItemView_ActivateItemOnSingleClick &&
            QCoreApplication::applicationName().compare(
                "dolphin", Qt::CaseInsensitive) == 0)
            return 0;
        return QProxyStyle::styleHint(hint, option, widget, data);
    }

  private:
    QFileSystemWatcher watcher;
    QTimer timer;
    QString source;
    QByteArray previous;
    void refresh() {
        QFile file(source);
        if (!file.open(QIODevice::ReadOnly) || file.size() > 1048576)
            return;
        QSettings settings(QDir::homePath() + "/.config/kdeglobals",
                           QSettings::IniFormat);
        settings.sync();
        const auto icons = settings.value("Icons/Theme").toString();
        const auto fontSetting = settings.value("General/font").toString();
        const auto fingerprint =
            file.readAll() + icons.toUtf8() + fontSetting.toUtf8();
        if (fingerprint == previous)
            return;
        const auto scheme = KSharedConfig::openConfig(source);
        scheme->reparseConfiguration();
        if (!scheme->hasGroup("Colors:View"))
            return;
        previous = fingerprint;
        qApp->setProperty("KDE_COLOR_SCHEME_PATH", source);
        const auto palette = KColorScheme::createApplicationPalette(scheme);
        QFont font;
        if (!fontSetting.isEmpty() && font.fromString(fontSetting))
            QApplication::setFont(font);
        QApplication::setPalette(palette);
        for (auto window : QApplication::topLevelWidgets())
            window->setPalette(palette);
        // Dolphin's graphics-view items cache style options separately from
        // QWidget. Notify style consumers after refreshing KDE's shared scheme
        // cache.
        QList<QPointer<QWidget>> widgets;
        for (auto widget : QApplication::allWidgets())
            widgets.append(widget);
        for (const auto &widget : widgets) {
            if (widget) {
                QEvent changed(QEvent::StyleChange);
                QCoreApplication::sendEvent(widget, &changed);
            }
        }
        if (!icons.isEmpty() && QIcon::themeName() != icons)
            QIcon::setThemeName(icons);
    }
};

class NacreDolphinPlugin : public QStylePlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "org.qt-project.Qt.QStyleFactoryInterface" FILE
                          "dolphin-style.json")
  public:
    QStyle *create(const QString &key) override {
        return key.compare("NacreDolphin", Qt::CaseInsensitive) == 0
                   ? new NacreDolphinStyle
                   : nullptr;
    }
};
#include "dolphin-style.moc"
