// SPDX-License-Identifier: BSD-3-Clause

#include <QJsonArray>
#include <QSignalSpy>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>
#include <QtTest>

#include "crypto.h"
#include "filetransferreceiver.h"
#include "filetransfersender.h"
#include "protocol.h"

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

// A WireHop receiver that negotiates the "ack" capability and then never
// delivers one, closing the socket instead. Note this is deliberately NOT a
// LANDrop 0.4.0 model: it inherits respond(), so it advertises caps. The
// capless peer shapes are emulated on raw sockets in
// legacyResponseUsesShortAckGrace and legacyMetadataStillTransfers.
class SilentAckReceiver : public FileTransferReceiver {
public:
    SilentAckReceiver(QObject *parent, QTcpSocket *socket, const QString &downloadPath) :
        FileTransferReceiver(parent, socket, downloadPath) {}
protected:
    void sendCompletionAck() override {}
};

class FastAckSender : public FileTransferSender {
public:
    FastAckSender(QObject *parent, QTcpSocket *socket, const QList<QSharedPointer<QFile>> &files,
                  const QString &deviceName) :
        FileTransferSender(parent, socket, files, deviceName) {}
protected:
    int watchdogIntervalMsecs() const override
    {
        if (state == WAITING_FOR_ACK)
            return 200;
        return FileTransferSender::watchdogIntervalMsecs();
    }
};

// Expose the negotiated peer state for assertions.
class ProbeSender : public FileTransferSender {
public:
    ProbeSender(QObject *parent, QTcpSocket *socket, const QList<QSharedPointer<QFile>> &files,
                const QString &deviceName) :
        FileTransferSender(parent, socket, files, deviceName) {}
    int peerVersion() const { return peerProtocolVersion; }
    bool peerAcksTransfers() const { return hasNegotiatedCap(Protocol::capAck()); }
};

class ProbeReceiver : public FileTransferReceiver {
public:
    ProbeReceiver(QObject *parent, QTcpSocket *socket, const QString &downloadPath) :
        FileTransferReceiver(parent, socket, downloadPath) {}
    int peerVersion() const { return peerProtocolVersion; }
    bool peerAcksTransfers() const { return hasNegotiatedCap(Protocol::capAck()); }
};

// Manual-peer helpers for emulating LANDrop 0.4.0 endpoints on a raw socket.
bool exchangeKeysManually(QTcpSocket *socket, Crypto &crypto)
{
    if (!QTest::qWaitFor([socket, &crypto]() {
            return static_cast<quint64>(socket->bytesAvailable()) >= crypto.publicKeySize();
        }, 5000))
        return false;
    crypto.setRemotePublicKey(socket->read(static_cast<qint64>(crypto.publicKeySize())));
    socket->write(crypto.localPublicKey());
    return true;
}

QByteArray makeFrame(Crypto &crypto, const QByteArray &plain)
{
    QByteArray frame = crypto.encrypt(plain);
    quint16 size = static_cast<quint16>(frame.size());
    frame.prepend(static_cast<char>(size & 0xFF));
    frame.prepend(static_cast<char>((size >> 8) & 0xFF));
    return frame;
}

QByteArray readNextFrame(QTcpSocket *socket, QByteArray &buffered, Crypto &crypto)
{
    while (true) {
        buffered += socket->readAll();
        if (buffered.size() >= 2) {
            quint16 size = static_cast<quint16>(static_cast<quint8>(buffered[0])) << 8;
            size |= static_cast<quint8>(buffered[1]);
            if (buffered.size() >= size + 2) {
                QByteArray frame = buffered.mid(2, size);
                buffered = buffered.mid(size + 2);
                // Never let a decrypt failure unwind through QTest: that
                // terminates the binary and loses every remaining result.
                try {
                    return crypto.decrypt(frame);
                } catch (const std::exception &) {
                    return QByteArray();
                }
            }
        }
        if (!QTest::qWaitFor([socket]() { return socket->bytesAvailable() > 0; }, 5000))
            return QByteArray();
    }
}

bool spyContainsMessage(const QSignalSpy &spy, const QString &message)
{
    for (int i = 0; i < spy.count(); ++i)
        if (spy.at(i).first().toString() == message)
            return true;
    return false;
}

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
    void unconfirmedWhenReceiverSkipsAck();
    void unconfirmedWhenAckNeverArrives();
    void capabilityNegotiationIsAdopted();
    void legacyResponseUsesShortAckGrace();
    void legacyMetadataStillTransfers();
    void oversizedCapsListIsTreatedAsLegacy();
    void rejectionFromPromptlyClosingPeerSurfacesOnce();
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

    QTRY_VERIFY(spyContainsMessage(senderMessageSpy, "Done!"));
    QVERIFY(!spyContainsMessage(senderMessageSpy,
                                "Sent, but the receiver did not confirm delivery."));
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
    QVERIFY(exchangeKeysManually(rawClient.data(), crypto));

    rawClient->write(makeFrame(crypto, "this is not json metadata"));

    QTRY_COMPARE(errorSpy.count(), 1);
    QVERIFY(QDir(downloadDir.path()).entryList(QDir::Files | QDir::Hidden).isEmpty());
}

void FileTransferSessionTest::unconfirmedWhenReceiverSkipsAck()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.txt"), QByteArray("legacy payload")));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<SilentAckReceiver> receiver(
            new SilentAckReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<FileTransferSender> sender(
            new FileTransferSender(nullptr, loop.clientSide, files, "test-device"));

    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
    QSignalSpy senderMessageSpy(sender.data(), &FileTransferSession::printMessage);
    QSignalSpy senderErrorSpy(sender.data(), &FileTransferSession::errorOccurred);

    receiver->start();
    sender->start();
    QTRY_COMPARE(metadataSpy.count(), 1);
    receiver->respond(true);

    // The receiver closes without acknowledging; the sender reports qualified
    // success rather than an error.
    QTRY_VERIFY(spyContainsMessage(senderMessageSpy,
                                   "Sent, but the receiver did not confirm delivery."));
    QVERIFY(!spyContainsMessage(senderMessageSpy, "Done!"));
    QCOMPARE(senderErrorSpy.count(), 0);

    QFile received(downloadDir.filePath("a.txt"));
    QVERIFY(received.open(QIODevice::ReadOnly));
    QCOMPARE(received.readAll(), QByteArray("legacy payload"));
}

void FileTransferSessionTest::unconfirmedWhenAckNeverArrives()
{
    QTemporaryDir sourceDir;
    QVERIFY(sourceDir.isValid());

    QByteArray content(1000, 'z');
    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.bin"), content));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<FastAckSender> sender(
            new FastAckSender(nullptr, loop.clientSide, files, "test-device"));
    QScopedPointer<QTcpSocket> rawReceiver(loop.serverSide);

    QSignalSpy senderMessageSpy(sender.data(), &FileTransferSession::printMessage);
    QSignalSpy senderErrorSpy(sender.data(), &FileTransferSession::errorOccurred);

    sender->start();

    // Drive a manual receiver that accepts and drains the transfer but never
    // acknowledges and never closes the connection.
    Crypto crypto;
    QVERIFY(exchangeKeysManually(rawReceiver.data(), crypto));

    QByteArray buffered;
    QByteArray metadata = readNextFrame(rawReceiver.data(), buffered, crypto);
    QVERIFY(QJsonDocument::fromJson(metadata).isObject());

    QJsonObject response;
    response.insert("response", 1);
    rawReceiver->write(makeFrame(crypto,
                                 QJsonDocument(response).toJson(QJsonDocument::Compact)));

    QByteArray receivedData;
    while (receivedData.size() < content.size()) {
        QByteArray chunk = readNextFrame(rawReceiver.data(), buffered, crypto);
        QVERIFY(!chunk.isEmpty());
        receivedData += chunk;
    }
    QCOMPARE(receivedData, content);

    // No acknowledgment and no close: the sender's ACK watchdog finishes the
    // session as unconfirmed instead of erroring.
    QTRY_VERIFY(spyContainsMessage(senderMessageSpy,
                                   "Sent, but the receiver did not confirm delivery."));
    QVERIFY(!spyContainsMessage(senderMessageSpy, "Done!"));
    QCOMPARE(senderErrorSpy.count(), 0);
}

void FileTransferSessionTest::capabilityNegotiationIsAdopted()
{
    QTemporaryDir sourceDir, downloadDir;
    QVERIFY(sourceDir.isValid());
    QVERIFY(downloadDir.isValid());

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.txt"), QByteArray("caps payload")));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<ProbeReceiver> receiver(
            new ProbeReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<ProbeSender> sender(
            new ProbeSender(nullptr, loop.clientSide, files, "test-device"));

    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
    QSignalSpy senderMessageSpy(sender.data(), &FileTransferSession::printMessage);

    receiver->start();
    sender->start();

    QTRY_COMPARE(metadataSpy.count(), 1);
    // The receiver adopts the sender's negotiation fields with the metadata.
    QCOMPARE(receiver->peerVersion(), static_cast<int>(Protocol::VERSION));
    QVERIFY(receiver->peerAcksTransfers());

    receiver->respond(true);

    QTRY_VERIFY(spyContainsMessage(senderMessageSpy, "Done!"));
    // The sender adopts the receiver's negotiation fields with the response.
    QCOMPARE(sender->peerVersion(), static_cast<int>(Protocol::VERSION));
    QVERIFY(sender->peerAcksTransfers());
}

void FileTransferSessionTest::legacyResponseUsesShortAckGrace()
{
    QTemporaryDir sourceDir;
    QVERIFY(sourceDir.isValid());

    QByteArray content(1000, 'q');
    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.bin"), content));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    // Real sender with the production acknowledgment windows.
    QScopedPointer<ProbeSender> sender(
            new ProbeSender(nullptr, loop.clientSide, files, "test-device"));
    QScopedPointer<QTcpSocket> rawReceiver(loop.serverSide);

    QSignalSpy senderMessageSpy(sender.data(), &FileTransferSession::printMessage);
    QSignalSpy senderErrorSpy(sender.data(), &FileTransferSession::errorOccurred);

    sender->start();

    Crypto crypto;
    QVERIFY(exchangeKeysManually(rawReceiver.data(), crypto));

    QByteArray buffered;
    QByteArray metadata = readNextFrame(rawReceiver.data(), buffered, crypto);
    QVERIFY(QJsonDocument::fromJson(metadata).isObject());

    // LANDrop 0.4.0 shape: a bare accept without negotiation fields.
    QJsonObject response;
    response.insert("response", 1);
    rawReceiver->write(makeFrame(crypto,
                                 QJsonDocument(response).toJson(QJsonDocument::Compact)));

    QByteArray receivedData;
    while (receivedData.size() < content.size()) {
        QByteArray chunk = readNextFrame(rawReceiver.data(), buffered, crypto);
        QVERIFY(!chunk.isEmpty());
        receivedData += chunk;
    }
    QCOMPARE(receivedData, content);
    QVERIFY(!sender->peerAcksTransfers());
    QCOMPARE(sender->peerVersion(), 0);

    // A capless peer only gets the short grace window (2 s), well inside the
    // 8 s ceiling below; the legacy 10 s window would fail this bound.
    QTRY_VERIFY_WITH_TIMEOUT(spyContainsMessage(senderMessageSpy,
            "Sent, but the receiver did not confirm delivery."), 8000);
    QVERIFY(!spyContainsMessage(senderMessageSpy, "Done!"));
    QCOMPARE(senderErrorSpy.count(), 0);
}

void FileTransferSessionTest::legacyMetadataStillTransfers()
{
    QTemporaryDir downloadDir;
    QVERIFY(downloadDir.isValid());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<ProbeReceiver> receiver(
            new ProbeReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<QTcpSocket> rawSender(loop.clientSide);

    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);
    QSignalSpy receiverErrorSpy(receiver.data(), &FileTransferSession::errorOccurred);

    receiver->start();

    Crypto crypto;
    QVERIFY(exchangeKeysManually(rawSender.data(), crypto));

    // LANDrop 0.4.0 shape: metadata without negotiation fields.
    QByteArray content("legacy sender bytes");
    QJsonObject file;
    file.insert("filename", "legacy.txt");
    file.insert("size", content.size());
    QJsonArray filesArray;
    filesArray.append(file);
    QJsonObject metadata;
    metadata.insert("device_name", "legacy-device");
    metadata.insert("device_type", "test");
    metadata.insert("files", filesArray);
    rawSender->write(makeFrame(crypto,
                               QJsonDocument(metadata).toJson(QJsonDocument::Compact)));

    QTRY_COMPARE(metadataSpy.count(), 1);
    QCOMPARE(receiver->peerVersion(), 0);
    QVERIFY(!receiver->peerAcksTransfers());

    receiver->respond(true);

    // The response to a legacy sender still advertises this build's version
    // and capabilities; a LANDrop 0.4.0 sender ignores the extra keys.
    QByteArray buffered;
    QByteArray responseData = readNextFrame(rawSender.data(), buffered, crypto);
    QJsonDocument responseJson = QJsonDocument::fromJson(responseData);
    QVERIFY(responseJson.isObject());
    QJsonObject responseObj = responseJson.object();
    QCOMPARE(responseObj.value("response").toInt(), 1);
    QCOMPARE(Protocol::parseVersion(responseObj.value("protocol_version")),
             static_cast<int>(Protocol::VERSION));
    QVERIFY(Protocol::parseCaps(responseObj.value("caps")).contains(Protocol::capAck()));

    rawSender->write(makeFrame(crypto, content));

    QTRY_VERIFY(QFileInfo::exists(downloadDir.filePath("legacy.txt")));
    QFile received(downloadDir.filePath("legacy.txt"));
    QVERIFY(received.open(QIODevice::ReadOnly));
    QCOMPARE(received.readAll(), content);

    // The completion acknowledgment is still sent; legacy senders ignore it.
    QByteArray ack = readNextFrame(rawSender.data(), buffered, crypto);
    QCOMPARE(QJsonDocument::fromJson(ack).object().value("ack").toInt(), 1);
    QCOMPARE(receiverErrorSpy.count(), 0);
}

void FileTransferSessionTest::oversizedCapsListIsTreatedAsLegacy()
{
    QTemporaryDir downloadDir;
    QVERIFY(downloadDir.isValid());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<ProbeReceiver> receiver(
            new ProbeReceiver(nullptr, loop.serverSide, downloadDir.path()));
    QScopedPointer<QTcpSocket> rawSender(loop.clientSide);

    QSignalSpy metadataSpy(receiver.data(), &FileTransferSession::fileMetadataReady);

    receiver->start();

    Crypto crypto;
    QVERIFY(exchangeKeysManually(rawSender.data(), crypto));

    QByteArray content("bounded caps");
    QJsonObject file;
    file.insert("filename", "bounded.txt");
    file.insert("size", content.size());
    QJsonArray filesArray;
    filesArray.append(file);
    QJsonArray oversizedCaps;
    for (int i = 0; i < Protocol::MAX_CAPS + 1; ++i)
        oversizedCaps.append(QString("cap%1").arg(i));
    QJsonObject metadata;
    metadata.insert("device_name", "noisy-device");
    metadata.insert("device_type", "test");
    metadata.insert("files", filesArray);
    metadata.insert("protocol_version", static_cast<int>(Protocol::VERSION));
    metadata.insert("caps", oversizedCaps);
    rawSender->write(makeFrame(crypto,
                               QJsonDocument(metadata).toJson(QJsonDocument::Compact)));

    QTRY_COMPARE(metadataSpy.count(), 1);
    // The version still parses; the out-of-bounds capability list is
    // discarded rather than failing the session.
    QCOMPARE(receiver->peerVersion(), static_cast<int>(Protocol::VERSION));
    QVERIFY(!receiver->peerAcksTransfers());

    receiver->respond(true);
    rawSender->write(makeFrame(crypto, content));
    QTRY_VERIFY(QFileInfo::exists(downloadDir.filePath("bounded.txt")));
}

void FileTransferSessionTest::rejectionFromPromptlyClosingPeerSurfacesOnce()
{
    QTemporaryDir sourceDir;
    QVERIFY(sourceDir.isValid());

    QList<QSharedPointer<QFile>> files;
    files.append(makeSourceFile(sourceDir.filePath("a.txt"), QByteArray("rejected")));
    QVERIFY(files.first());

    Loopback loop;
    QVERIFY(connectLoopback(loop));
    QScopedPointer<ProbeSender> sender(
            new ProbeSender(nullptr, loop.clientSide, files, "test-device"));
    QScopedPointer<QTcpSocket> rawReceiver(loop.serverSide);

    QSignalSpy errorSpy(sender.data(), &FileTransferSession::errorOccurred);
    sender->start();

    Crypto crypto;
    QVERIFY(exchangeKeysManually(rawReceiver.data(), crypto));

    QByteArray buffered;
    QVERIFY(!readNextFrame(rawReceiver.data(), buffered, crypto).isEmpty());

    // Decline and close at once, as any peer may. The reason the user needs
    // must not then be overwritten by a generic socket error.
    QJsonObject response;
    response.insert("response", 0);
    rawReceiver->write(makeFrame(crypto,
                                 QJsonDocument(response).toJson(QJsonDocument::Compact)));
    QVERIFY(rawReceiver->waitForBytesWritten(5000));
    rawReceiver->close();

    QTRY_COMPARE(errorSpy.count(), 1);
    QCOMPARE(errorSpy.at(0).first().toString(),
             QStringLiteral("The receiving device rejected your file(s)."));

    // Give a spurious follow-up error time to appear if the fix regresses.
    QTest::qWait(500);
    QCOMPARE(errorSpy.count(), 1);
}

int runFileTransferSessionTest(int argc, char *argv[])
{
    FileTransferSessionTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_filetransfersession.moc"
