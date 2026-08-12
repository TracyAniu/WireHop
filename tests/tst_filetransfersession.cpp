// SPDX-License-Identifier: BSD-3-Clause

#include <QSignalSpy>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>
#include <QtTest>

#include "crypto.h"
#include "filetransferreceiver.h"
#include "filetransfersender.h"

Q_DECLARE_METATYPE(QList<FileTransferSession::FileMetadata>)

namespace {

class FastTimeoutReceiver : public FileTransferReceiver {
public:
    FastTimeoutReceiver(QObject *parent, QTcpSocket *socket, const QString &downloadPath) :
        FileTransferReceiver(parent, socket, downloadPath) {}
protected:
    int watchdogIntervalMsecs() const override
    {
        return FileTransferReceiver::watchdogIntervalMsecs() > 0 ? 200 : 0;
    }
};

}

class FileTransferSessionTest : public QObject {
    Q_OBJECT
private slots:
    void initTestCase();
    void acceptedTransferDeliversFiles();
    void rejectedTransferSurfacesError();
    void midTransferDisconnectLeavesNoPartials();
    void repeatedRespondIsRejected();
    void idlePeerTimesOut();
    void malformedMetadataIsRejected();
private:
    struct Loopback {
        QTcpServer server;
        QTcpSocket *clientSide = nullptr;
        QTcpSocket *serverSide = nullptr;
    };
    bool connectLoopback(Loopback &loop);
    QSharedPointer<QFile> makeSourceFile(const QString &path, const QByteArray &content);
};

void FileTransferSessionTest::initTestCase()
{
    qRegisterMetaType<QList<FileTransferSession::FileMetadata>>(
            "QList<FileTransferSession::FileMetadata>");
}

bool FileTransferSessionTest::connectLoopback(Loopback &loop)
{
    if (!loop.server.listen(QHostAddress::LocalHost, 0))
        return false;
    loop.clientSide = new QTcpSocket;
    loop.clientSide->connectToHost(QHostAddress::LocalHost, loop.server.serverPort());
    if (!loop.server.waitForNewConnection(5000))
        return false;
    loop.serverSide = loop.server.nextPendingConnection();
    return loop.serverSide && loop.clientSide->waitForConnected(5000);
}

QSharedPointer<QFile> FileTransferSessionTest::makeSourceFile(const QString &path,
                                                              const QByteArray &content)
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

void FileTransferSessionTest::acceptedTransferDeliversFiles()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QByteArray smallContent("hello loopback");
    QByteArray bigContent(200000, 'x');
    for (int i = 0; i < bigContent.size(); i += 977)
        bigContent[i] = static_cast<char>('a' + (i % 26));

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.txt"), smallContent));
    files.append(makeSourceFile(sourceDir.filePath("b.bin"), bigContent));
    files.append(makeSourceFile(sourceDir.filePath("empty.dat"), QByteArray()));
    foreach (const QSharedPointer<QFile> &file, files)
        QVERIFY(file);

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<FileTransferReceiver> receiver(
            new FileTransferReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<FileTransferSender> sender(
            new FileTransferSender(nullptr, loop.clientSide, files, "test-device"));

    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
    QSignalSpy receiverErrorSpy(receiver.data(), &FileTransferSession::errorOccurred);
    QSignalSpy senderErrorSpy(sender.data(), &FileTransferSession::errorOccurred);
    QSignalSpy senderMessageSpy(sender.data(), &FileTransferSession::printMessage);
    QSignalSpy openFolderSpy(receiver.data(), &FileTransferSession::openDownloadFolder);

    receiver->start();
    sender->start();

    QTRY_COMPARE(metadataSpy.count(), 1);
    QList<QVariant> metadataArgs = metadataSpy.takeFirst();
    QList<FileTransferSession::FileMetadata> metadata =
            metadataArgs.at(0).value<QList<FileTransferSession::FileMetadata>>();
    QCOMPARE(metadata.size(), 3);
    QCOMPARE(metadata.at(0).filename, QString("a.txt"));
    QCOMPARE(metadataArgs.at(1).toULongLong(),
             static_cast<quint64>(smallContent.size() + bigContent.size()));
    QCOMPARE(metadataArgs.at(2).toString(), QString("test-device"));

    receiver->respond(true);

    QTRY_COMPARE(openFolderSpy.count(), 1);
    QCOMPARE(openFolderSpy.first().first().toString(), downloadDir.path());

    QFile receivedSmall(downloadDir.filePath("a.txt"));
    QVERIFY(receivedSmall.open(QIODevice::ReadOnly));
    QCOMPARE(receivedSmall.readAll(), smallContent);
    QFile receivedBig(downloadDir.filePath("b.bin"));
    QVERIFY(receivedBig.open(QIODevice::ReadOnly));
    QCOMPARE(receivedBig.readAll(), bigContent);
    QVERIFY(QFileInfo::exists(downloadDir.filePath("empty.dat")));
    QCOMPARE(QFileInfo(downloadDir.filePath("empty.dat")).size(), qint64(0));

    QTRY_VERIFY([&]() {
        for (int i = 0; i < senderMessageSpy.count(); ++i)
            if (senderMessageSpy.at(i).first().toString() == "Done!")
                return true;
        return false;
    }());
    QCOMPARE(receiverErrorSpy.count(), 0);
    QCOMPARE(senderErrorSpy.count(), 0);

    QStringList leftovers = QDir(downloadDir.path())
            .entryList(QStringList() << ".wirehop-part-*", QDir::Files | QDir::Hidden);
    QVERIFY(leftovers.isEmpty());
}

void FileTransferSessionTest::rejectedTransferSurfacesError()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.txt"), QByteArray("payload")));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<FileTransferReceiver> receiver(
            new FileTransferReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<FileTransferSender> sender(
            new FileTransferSender(nullptr, loop.clientSide, files, "test-device"));

    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
    QSignalSpy senderErrorSpy(sender.data(), &FileTransferSession::errorOccurred);
    QSignalSpy receiverEndedSpy(receiver.data(), &FileTransferSession::ended);

    receiver->start();
    sender->start();

    QTRY_COMPARE(metadataSpy.count(), 1);
    receiver->respond(false);

    QTRY_COMPARE(senderErrorSpy.count(), 1);
    QTRY_COMPARE(receiverEndedSpy.count(), 1);
    QVERIFY(QDir(downloadDir.path()).entryList(QDir::Files | QDir::Hidden).isEmpty());
}

void FileTransferSessionTest::midTransferDisconnectLeavesNoPartials()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("big.bin"), QByteArray(4 * 1024 * 1024, 'y')));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    {
        QScopedPointer<FileTransferReceiver> receiver(
                new FileTransferReceiver(nullptr, loop.serverSide, downloadDir.path()));
        QScopedPointer<FileTransferSender> sender(
                new FileTransferSender(nullptr, loop.clientSide, files, "test-device"));

        QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
        QSignalSpy receiverErrorSpy(receiver.data(), &FileTransferSession::errorOccurred);

        // Abort the sending side synchronously on the first receiver progress
        // signal, guaranteeing the disconnect lands mid-transfer.
        bool aborted = false;
        connect(receiver.data(), &FileTransferSession::updateProgress, loop.clientSide,
                [&aborted, &loop](double) {
            if (!aborted) {
                aborted = true;
                loop.clientSide->abort();
            }
        });

        receiver->start();
        sender->start();
        QTRY_COMPARE(metadataSpy.count(), 1);
        receiver->respond(true);

        QTRY_COMPARE(receiverErrorSpy.count(), 1);
        QVERIFY(aborted);
        QVERIFY(!QFileInfo::exists(downloadDir.filePath("big.bin")));
    }
    // Receiver destruction removes the auto-remove temporary file.
    QVERIFY(QDir(downloadDir.path()).entryList(QDir::Files | QDir::Hidden).isEmpty());
}

void FileTransferSessionTest::repeatedRespondIsRejected()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.txt"), QByteArray("payload")));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<FileTransferReceiver> receiver(
            new FileTransferReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<FileTransferSender> sender(
            new FileTransferSender(nullptr, loop.clientSide, files, "test-device"));

    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
    QSignalSpy receiverErrorSpy(receiver.data(), &FileTransferSession::errorOccurred);
    QSignalSpy senderErrorSpy(sender.data(), &FileTransferSession::errorOccurred);

    receiver->start();
    sender->start();
    QTRY_COMPARE(metadataSpy.count(), 1);

    receiver->respond(true);
    receiver->respond(true);

    // The second respond is rejected without sending a second response frame:
    // the receiver reports a local error and the transfer still completes.
    QCOMPARE(receiverErrorSpy.count(), 1);
    QTRY_VERIFY(QFileInfo::exists(downloadDir.filePath("a.txt")));
    QFile received(downloadDir.filePath("a.txt"));
    QVERIFY(received.open(QIODevice::ReadOnly));
    QCOMPARE(received.readAll(), QByteArray("payload"));
    QCOMPARE(senderErrorSpy.count(), 0);
}

void FileTransferSessionTest::idlePeerTimesOut()
{
    QTemporaryDir downloadDir;
    QVERIFY(downloadDir.isValid());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<FastTimeoutReceiver> receiver(
            new FastTimeoutReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<QTcpSocket> silentClient(loop.clientSide);

    QSignalSpy errorSpy(receiver.data(), &FileTransferSession::errorOccurred);
    receiver->start();

    QTRY_COMPARE_WITH_TIMEOUT(errorSpy.count(), 1, 5000);
    QCOMPARE(errorSpy.first().first().toString(), QString("The connection timed out."));
    QTRY_VERIFY(silentClient->state() == QAbstractSocket::UnconnectedState);
}

void FileTransferSessionTest::malformedMetadataIsRejected()
{
    QTemporaryDir downloadDir;
    QVERIFY(downloadDir.isValid());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<FileTransferReceiver> receiver(
            new FileTransferReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<QTcpSocket> rawClient(loop.clientSide);

    QSignalSpy errorSpy(receiver.data(), &FileTransferSession::errorOccurred);
    receiver->start();

    Crypto crypto;
    QTRY_VERIFY(static_cast<quint64>(rawClient->bytesAvailable()) >= crypto.publicKeySize());
    crypto.setRemotePublicKey(rawClient->read(static_cast<qint64>(crypto.publicKeySize())));
    rawClient->write(crypto.localPublicKey());

    QByteArray frame = crypto.encrypt("this is not json metadata");
    quint16 size = static_cast<quint16>(frame.size());
    frame.prepend(static_cast<char>(size & 0xFF));
    frame.prepend(static_cast<char>((size >> 8) & 0xFF));
    rawClient->write(frame);

    QTRY_COMPARE(errorSpy.count(), 1);
    QVERIFY(QDir(downloadDir.path()).entryList(QDir::Files | QDir::Hidden).isEmpty());
}

int runFileTransferSessionTest(int argc, char *argv[])
{
    FileTransferSessionTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_filetransfersession.moc"
