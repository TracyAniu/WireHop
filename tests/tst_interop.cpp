// SPDX-License-Identifier: BSD-3-Clause

// Live interoperability gate: a real encrypted transfer between the C++/Qt
// implementation and the Rust core, in both directions, over loopback.
//
// The conformance vectors (tst_protocolvectors.cpp) prove the two
// implementations *agree about* the protocol. This proves two independent
// processes can actually complete a transfer with each other — framing,
// chunking, file boundaries, negotiation, and the acknowledgment, end to end.
//
// The Rust binary is located via WIREHOP_CLI_BIN (set by scripts/test.sh after
// `cargo build`). When it is absent the suite skips rather than fails, so a
// contributor without a Rust toolchain is not blocked; CI sets
// WIREHOP_REQUIRE_RUST=1, which turns the skip into a failure there.

#include <QDir>
#include <QProcess>
#include <QSignalSpy>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>
#include <QtTest>

#include "filetransferreceiver.h"
#include "filetransfersender.h"
#include "protocol.h"

Q_DECLARE_METATYPE(QList<FileTransferSession::FileMetadata>)

namespace {

QString cliBinary()
{
    return qEnvironmentVariable("WIREHOP_CLI_BIN");
}

bool rustRequired()
{
    return qEnvironmentVariable("WIREHOP_REQUIRE_RUST") == QStringLiteral("1");
}

QSharedPointer<QFile> makeSourceFile(const QString &path, const QByteArray &content)
{
    QFile writer(path);
    if (!writer.open(QIODevice::WriteOnly))
        return QSharedPointer<QFile>();
    if (writer.write(content) != content.size())
        return QSharedPointer<QFile>();
    writer.close();

    QSharedPointer<QFile> file = QSharedPointer<QFile>::create(path);
    if (!file->open(QIODevice::ReadOnly))
        return QSharedPointer<QFile>();
    return file;
}

bool spyContainsMessage(const QSignalSpy &spy, const QString &message)
{
    for (int i = 0; i < spy.count(); ++i)
        if (spy.at(i).first().toString() == message)
            return true;
    return false;
}

// A payload big enough to span many frames, with position-dependent bytes so a
// reordering or off-by-one corrupts the comparison rather than hiding in a run
// of identical bytes.
QByteArray patternedPayload(int size)
{
    QByteArray data(size, 0);
    for (int i = 0; i < size; ++i)
        data[i] = static_cast<char>((i * 31 + (i / 251)) & 0xFF);
    return data;
}

} // namespace

class InteropTest : public QObject {
    Q_OBJECT
private slots:
    void initTestCase();
    void rustSenderToQtReceiver();
    void qtSenderToRustReceiver();
    void rustReceiverRejectionSurfacesToQtSender();
private:
    QString cli;
};

void InteropTest::initTestCase()
{
    qRegisterMetaType<QList<FileTransferSession::FileMetadata>>(
            "QList<FileTransferSession::FileMetadata>");

    cli = cliBinary();
    if (cli.isEmpty() || !QFileInfo::exists(cli)) {
        QString reason = QStringLiteral(
                "WIREHOP_CLI_BIN is unset or missing; build it with "
                "`cargo build -p wirehop-cli` or run ./scripts/test.sh");
        if (rustRequired())
            QFAIL(qPrintable(reason + " (WIREHOP_REQUIRE_RUST=1)"));
        QSKIP(qPrintable(reason));
    }
}

void InteropTest::rustSenderToQtReceiver()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    // Sizes chosen so the transfer is not frame-aligned: the last frame of the
    // first file is partial, and a zero-byte file contributes no frames.
    QByteArray small("hello from rust");
    QByteArray big = patternedPayload(150000);
    QVERIFY(QFile(sourceDir.filePath("a.txt")).open(QIODevice::WriteOnly));
    {
        QFile f(sourceDir.filePath("a.txt"));
        QVERIFY(f.open(QIODevice::WriteOnly));
        QCOMPARE(f.write(small), static_cast<qint64>(small.size()));
    }
    {
        QFile f(sourceDir.filePath("b.bin"));
        QVERIFY(f.open(QIODevice::WriteOnly));
        QCOMPARE(f.write(big), static_cast<qint64>(big.size()));
    }
    {
        QFile f(sourceDir.filePath("empty.dat"));
        QVERIFY(f.open(QIODevice::WriteOnly));
    }

    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));

    QProcess sender;
    sender.start(cli, {"send", "--port", QString::number(server.serverPort()),
                       "--name", "rust-peer",
                       sourceDir.filePath("a.txt"),
                       sourceDir.filePath("b.bin"),
                       sourceDir.filePath("empty.dat")});
    QVERIFY2(sender.waitForStarted(5000), qPrintable(sender.errorString()));

    QVERIFY(server.waitForNewConnection(10000));
    QTcpSocket *peer = server.nextPendingConnection();
    QVERIFY(peer);

    QScopedPointer<FileTransferReceiver> receiver(
            new FileTransferReceiver(nullptr, peer, downloadDir.path()));
    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
    QSignalSpy messageSpy(receiver.data(), &FileTransferSession::printMessage);
    QSignalSpy errorSpy(receiver.data(), &FileTransferSession::errorOccurred);

    receiver->start();
    QTRY_VERIFY_WITH_TIMEOUT(metadataSpy.count() == 1, 15000);

    // The Rust peer's identity and negotiation reached this implementation.
    QCOMPARE(metadataSpy.at(0).at(2).toString(), QStringLiteral("rust-peer"));
    QList<FileTransferSession::FileMetadata> metadata =
            metadataSpy.at(0).at(0).value<QList<FileTransferSession::FileMetadata>>();
    QCOMPARE(metadata.size(), 3);
    QCOMPARE(metadata.at(0).filename, QStringLiteral("a.txt"));
    QCOMPARE(metadata.at(1).size, static_cast<quint64>(big.size()));
    QCOMPARE(metadata.at(2).size, static_cast<quint64>(0));

    receiver->respond(true);
    QTRY_VERIFY_WITH_TIMEOUT(spyContainsMessage(messageSpy, QStringLiteral("Done!")), 30000);
    QCOMPARE(errorSpy.count(), 0);

    // Byte-for-byte delivery.
    QFile receivedSmall(QDir(downloadDir.path()).filePath("a.txt"));
    QVERIFY(receivedSmall.open(QIODevice::ReadOnly));
    QCOMPARE(receivedSmall.readAll(), small);

    QFile receivedBig(QDir(downloadDir.path()).filePath("b.bin"));
    QVERIFY(receivedBig.open(QIODevice::ReadOnly));
    QCOMPARE(receivedBig.readAll(), big);

    QFile receivedEmpty(QDir(downloadDir.path()).filePath("empty.dat"));
    QVERIFY(receivedEmpty.open(QIODevice::ReadOnly));
    QCOMPARE(receivedEmpty.size(), 0);

    // No partial files survive a completed transfer.
    QStringList leftovers = QDir(downloadDir.path())
                                    .entryList(QStringList() << ".wirehop-part-*",
                                               QDir::Files | QDir::Hidden);
    QVERIFY2(leftovers.isEmpty(), qPrintable(leftovers.join(", ")));

    // Spin the event loop while waiting: waitForFinished() would block it,
    // leaving the acknowledgment queued in the socket's write buffer while the
    // Rust peer waits for exactly that byte — a deadlock of our own making.
    QTRY_VERIFY_WITH_TIMEOUT(sender.state() == QProcess::NotRunning, 20000);
    QCOMPARE(sender.exitCode(), 0);
    // The Qt receiver's acknowledgment was understood by the Rust sender.
    QVERIFY2(QString::fromUtf8(sender.readAllStandardOutput()).contains("outcome confirmed"),
             qPrintable(QString::fromUtf8(sender.readAllStandardError())));
}

void InteropTest::qtSenderToRustReceiver()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QByteArray content = patternedPayload(120000);
    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("from-qt.bin"), content));
    files.append(makeSourceFile(sourceDir.filePath("零字节.dat"), QByteArray()));
    foreach (const QSharedPointer<QFile> &file, files)
        QVERIFY(file);

    QProcess receiver;
    receiver.start(cli, {"receive", "--port", "0", "--dir", downloadDir.path()});
    QVERIFY2(receiver.waitForStarted(5000), qPrintable(receiver.errorString()));

    // The child prints its bound port before it blocks on accept().
    QVERIFY2(receiver.waitForReadyRead(10000), "rust receiver never announced a port");
    QString announcement = QString::fromUtf8(receiver.readLine()).trimmed();
    QVERIFY2(announcement.startsWith(QStringLiteral("listening ")),
             qPrintable(announcement));
    bool ok = false;
    quint16 port = announcement.mid(10).toUShort(&ok);
    QVERIFY(ok && port != 0);

    QTcpSocket *socket = new QTcpSocket;
    socket->connectToHost(QHostAddress::LocalHost, port);
    QVERIFY(socket->waitForConnected(10000));

    QScopedPointer<FileTransferSender> sender(
            new FileTransferSender(nullptr, socket, files, "qt-peer"));
    QSignalSpy messageSpy(sender.data(), &FileTransferSession::printMessage);
    QSignalSpy errorSpy(sender.data(), &FileTransferSession::errorOccurred);

    sender->start();

    // "Done!" here means the Rust receiver's acknowledgment was received and
    // understood, which only happens when `ack` negotiated successfully.
    QTRY_VERIFY_WITH_TIMEOUT(spyContainsMessage(messageSpy, QStringLiteral("Done!")), 30000);
    QCOMPARE(errorSpy.count(), 0);

    QTRY_VERIFY_WITH_TIMEOUT(receiver.state() == QProcess::NotRunning, 20000);
    QCOMPARE(receiver.exitCode(), 0);

    QString report = QString::fromUtf8(receiver.readAllStandardOutput());
    QVERIFY2(report.contains(QStringLiteral("device qt-peer")), qPrintable(report));
    QVERIFY2(report.contains(QStringLiteral("version 1")), qPrintable(report));
    QVERIFY2(report.contains(QStringLiteral("caps ack")), qPrintable(report));
    QVERIFY2(report.contains(QStringLiteral("outcome accepted")), qPrintable(report));

    QFile delivered(QDir(downloadDir.path()).filePath("from-qt.bin"));
    QVERIFY(delivered.open(QIODevice::ReadOnly));
    QCOMPARE(delivered.readAll(), content);

    // Non-ASCII filenames survive the round trip intact.
    QFile empty(QDir(downloadDir.path()).filePath(QString::fromUtf8("零字节.dat")));
    QVERIFY(empty.open(QIODevice::ReadOnly));
    QCOMPARE(empty.size(), 0);
}

void InteropTest::rustReceiverRejectionSurfacesToQtSender()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.txt"), QByteArray("rejected payload")));
    QVERIFY(files.first());

    QProcess receiver;
    receiver.start(cli, {"receive", "--port", "0", "--dir", downloadDir.path(), "--reject"});
    QVERIFY2(receiver.waitForStarted(5000), qPrintable(receiver.errorString()));
    QVERIFY(receiver.waitForReadyRead(10000));
    QString announcement = QString::fromUtf8(receiver.readLine()).trimmed();
    bool ok = false;
    quint16 port = announcement.mid(10).toUShort(&ok);
    QVERIFY(ok && port != 0);

    QTcpSocket *socket = new QTcpSocket;
    socket->connectToHost(QHostAddress::LocalHost, port);
    QVERIFY(socket->waitForConnected(10000));

    QScopedPointer<FileTransferSender> sender(
            new FileTransferSender(nullptr, socket, files, "qt-peer"));
    QSignalSpy errorSpy(sender.data(), &FileTransferSession::errorOccurred);
    sender->start();

    // A decline must reach the user as a rejection, not a generic failure.
    QTRY_VERIFY_WITH_TIMEOUT(errorSpy.count() >= 1, 20000);
    QStringList errors;
    for (int i = 0; i < errorSpy.count(); ++i)
        errors << errorSpy.at(i).first().toString();
    // Exactly one, and it must be the reason — not a socket error that landed
    // afterwards because the Rust peer closed as soon as it declined.
    QVERIFY2(errors == QStringList{QStringLiteral("The receiving device rejected your file(s).")},
             qPrintable(QStringLiteral("errors were: %1").arg(errors.join(" | "))));

    QTRY_VERIFY_WITH_TIMEOUT(receiver.state() == QProcess::NotRunning, 20000);
    QVERIFY(QDir(downloadDir.path()).entryList(QDir::Files | QDir::Hidden).isEmpty());
}

int runInteropTest(int argc, char *argv[])
{
    InteropTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_interop.moc"
