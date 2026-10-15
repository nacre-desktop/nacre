// Developer-only Qt Widgets paint/flush witness. Never installed with Nacre.
#include <QApplication>
#include <QDateTime>
#include <QElapsedTimer>
#include <QEvent>
#include <QListWidget>
#include <QVBoxLayout>
#include <QWidget>
#include <cstdio>

class Witness final : public QWidget {
    QString previous;
    QElapsedTimer clock;
public:
    Witness() {
        clock.start();
        setWindowTitle("Nacre palette Qt witness");
        resize(320, 160);
        auto *layout = new QVBoxLayout(this);
        auto *list = new QListWidget(this);
        list->addItem("Palette paint witness");
        list->setCurrentRow(0);
        layout->addWidget(list);
    }
    bool event(QEvent *event) override {
        const bool result = QWidget::event(event);
        if (event->type() == QEvent::UpdateRequest) {
            const QString color = palette().color(QPalette::Active, QPalette::Highlight).name().mid(1);
            if (color != previous) {
                previous = color;
                std::printf("{\"consumer\":\"qt6\",\"primary\":\"%s\","
                            "\"monotonic_ms\":%lld,\"realtime_ms\":%lld,"
                            "\"boundary\":\"QWidget.UpdateRequest.after-flush\"}\n",
                            color.toUtf8().constData(),
                            static_cast<long long>(clock.msecsSinceReference() + clock.elapsed()),
                            static_cast<long long>(QDateTime::currentMSecsSinceEpoch()));
                std::fflush(stdout);
            }
        }
        return result;
    }
};

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    Witness witness;
    witness.show();
    return app.exec();
}
