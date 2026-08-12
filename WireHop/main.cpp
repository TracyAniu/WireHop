/*
 * BSD 3-Clause License
 *
 * Copyright (c) 2021, LANDrop
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * 1. Redistributions of source code must retain the above copyright notice, this
 *    list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright notice,
 *    this list of conditions and the following disclaimer in the documentation
 *    and/or other materials provided with the distribution.
 *
 * 3. Neither the name of the copyright holder nor the names of its
 *    contributors may be used to endorse or promote products derived from
 *    this software without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 * AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
 * SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 * CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

#include <stdexcept>

#include <QApplication>
#include <QFileInfo>
#include <QFileOpenEvent>
#include <QMessageBox>
#include <QTimer>
#include <QTranslator>

#include "settings.h"
#include "trayicon.h"

#ifdef Q_OS_MACOS
#include "macservices.h"
#endif

// Collects QFileOpenEvent paths (delivered to the application object) and
// forwards them to the tray in one batch. macOS sends one event per file, so
// a zero-delay timer coalesces a multi-file open into a single send dialog.
class FileOpenCollector : public QObject {
public:
    void setTrayIcon(TrayIcon *icon)
    {
        trayIcon = icon;
        if (!pending.isEmpty())
            scheduleFlush();
    }
protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if (event->type() == QEvent::FileOpen) {
            QFileOpenEvent *openEvent = static_cast<QFileOpenEvent *>(event);
            QString path = openEvent->file();
            if (path.isEmpty() && openEvent->url().isLocalFile())
                path = openEvent->url().toLocalFile();
            if (!path.isEmpty()) {
                pending.append(path);
                scheduleFlush();
            }
            return true;
        }
        return QObject::eventFilter(watched, event);
    }
private:
    void scheduleFlush()
    {
        if (flushScheduled || !trayIcon)
            return;
        flushScheduled = true;
        QTimer::singleShot(0, this, [this]() {
            flushScheduled = false;
            if (!trayIcon || pending.isEmpty())
                return;
            QStringList batch = pending;
            pending.clear();
            trayIcon->sendFiles(batch);
        });
    }
    TrayIcon *trayIcon = nullptr;
    QStringList pending;
    bool flushScheduled = false;
};

int main(int argc, char *argv[])
{
    QApplication a(argc, argv);

    a.setOrganizationName("WireHop");
    a.setOrganizationDomain("tracyaniu.github.io");
    a.setApplicationName("WireHop");
    a.setApplicationVersion("0.1.0");

    Settings::migrateLegacySettings();

    a.setQuitOnLastWindowClosed(false);

    QTranslator appTranslator;
    appTranslator.load(a.applicationName() + '.' + QLocale::system().name(), ":/locales", "", ".qm");
    a.installTranslator(&appTranslator);

    FileOpenCollector fileOpenCollector;
    a.installEventFilter(&fileOpenCollector);

    try {
        if (!QSystemTrayIcon::isSystemTrayAvailable())
            throw std::runtime_error(a.translate("Main", "Your system needs to support tray icon.")
                                     .toUtf8().toStdString());

        TrayIcon t;
        t.show();
        fileOpenCollector.setTrayIcon(&t);
#ifdef Q_OS_MACOS
        registerMacServicesProvider(&t);
#endif

        QStringList cliFiles;
        foreach (const QString &argument, a.arguments().mid(1)) {
            if (QFileInfo::exists(argument))
                cliFiles.append(argument);
        }
        if (!cliFiles.isEmpty())
            t.sendFiles(cliFiles);

        return a.exec();
    } catch (const std::exception &e) {
        QMessageBox::critical(nullptr, QApplication::applicationName(), e.what());
        return 1;
    }
}
