#include <QApplication>
#include <QEventLoop>
#include <QFile>
#include <QPalette>
#include <QTimer>
#include <QWidget>
#include <cstdio>
static void settle() {
    QEventLoop loop;
    QTimer::singleShot(150, &loop, &QEventLoop::quit);
    loop.exec();
}
int main(int argc, char **argv) {
    QApplication app(argc, argv);
    app.setApplicationName(argc > 1 ? "unrelated" : "dolphin");
    QWidget view;
    view.show();
    settle();
    const auto first = view.palette().color(QPalette::Highlight);
    const auto filename = qEnvironmentVariable("HOME") +
                          "/.local/share/color-schemes/Nacre.colors";
    QFile file(filename);
    if (!file.open(QIODevice::ReadOnly))
        return 2;
    auto text = file.readAll();
    file.close();
    text.replace("#cc5555", "#55aa99");
    QFile replacement(filename + ".next");
    if (!replacement.open(QIODevice::WriteOnly))
        return 3;
    replacement.write(text);
    replacement.close();
    QFile::remove(filename);
    if (!replacement.rename(filename))
        return 4;
    settle();
    const auto second = view.palette().color(QPalette::Highlight);
    if (argc > 1) {
        if (first != second)
            return 5;
    } else {
        if (first != QColor("#cc5555") || second != QColor("#55aa99"))
            return 6;
    }
    std::puts("Scoped native palette replacement passed");
    return 0;
}
