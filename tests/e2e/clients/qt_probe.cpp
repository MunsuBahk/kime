// Qt5/Qt6 QLineEdit probe for the kime e2e harness (compiled twice by cc.rs,
// once against Qt5Widgets and once against Qt6Widgets).
//
// Usage: qt_probe <outfile>
//   <outfile>          line-edit text, rewritten every 150ms
//   <outfile>.preedit  appended "p:<preedit>" / "c:<commit>" per QInputMethodEvent
// Prints READY on stderr once the window is shown.
//
// The event filter makes preedit observable (QLineEdit::text() only shows
// committed text), which the candidate-window (#757) and plugin-loading
// (#736/#756) tests rely on.
#include <QApplication>
#include <QFile>
#include <QInputMethodEvent>
#include <QLineEdit>
#include <QTimer>
#include <cstdio>

class ImLogger : public QObject {
public:
    explicit ImLogger(const QString &path) : m_path(path) {}

protected:
    bool eventFilter(QObject *obj, QEvent *ev) override {
        if (ev->type() == QEvent::InputMethod) {
            auto *ime = static_cast<QInputMethodEvent *>(ev);
            QFile f(m_path);
            if (f.open(QIODevice::Append)) {
                f.write("p:" + ime->preeditString().toUtf8() + "\n");
                if (!ime->commitString().isEmpty()) {
                    f.write("c:" + ime->commitString().toUtf8() + "\n");
                }
            }
        }
        return QObject::eventFilter(obj, ev);
    }

private:
    QString m_path;
};

int main(int argc, char **argv) {
    QApplication a(argc, argv);
    if (argc < 2) {
        fprintf(stderr, "usage: qt_probe <outfile>\n");
        return 1;
    }
    QString out = QString::fromLocal8Bit(argv[1]);
    QLineEdit e;
    e.setWindowTitle("kimeprobe");
    e.resize(320, 60);
    ImLogger logger(out + ".preedit");
    e.installEventFilter(&logger);
    e.show();
    QTimer t;
    QObject::connect(&t, &QTimer::timeout, [&] {
        QFile f(out);
        if (f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
            f.write(e.text().toUtf8());
        }
    });
    t.start(150);
    fprintf(stderr, "READY\n");
    fflush(stderr);
    return a.exec();
}
