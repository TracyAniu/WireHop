// SPDX-License-Identifier: BSD-3-Clause

#include <QFile>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QtTest>

#include <cmath>
#include <limits>
#include <stdexcept>

#include "crypto.h"
#include "filetransferpolicy.h"

class FileTransferPolicyTest : public QObject {
    Q_OBJECT
private slots:
    void acceptsPortableFilenames();
    void rejectsUnsafeFilenames_data();
    void rejectsUnsafeFilenames();
    void validatesDeviceNames();
    void validatesFileSizes();
    void validatesPorts();
    void enforcesTotalSize();
    void generatesCollisionPaths();
    void commitsWithoutOverwriting();
    void exchangesEncryptedMessages();
    void rejectsInvalidCryptoInputs();
};

void FileTransferPolicyTest::acceptsPortableFilenames()
{
    QVERIFY(FileTransferPolicy::isSafeFilename("photo.jpg"));
    QVERIFY(FileTransferPolicy::isSafeFilename("archive.tar.gz"));
    QVERIFY(FileTransferPolicy::isSafeFilename("报告 2026.pdf"));
    QVERIFY(FileTransferPolicy::isSafeFilename(".hidden"));
    QVERIFY(FileTransferPolicy::isSafeFilename(QString::fromUtf8("photo-😀.jpg")));
}

void FileTransferPolicyTest::rejectsUnsafeFilenames_data()
{
    QTest::addColumn<QString>("filename");

    QTest::newRow("empty") << QString();
    QTest::newRow("dot") << QString(".");
    QTest::newRow("dot-dot") << QString("..");
    QTest::newRow("relative-traversal") << QString("../secret");
    QTest::newRow("forward-slash") << QString("folder/file.txt");
    QTest::newRow("backslash") << QString("folder\\file.txt");
    QTest::newRow("absolute") << QString("/tmp/file.txt");
    QTest::newRow("drive-prefix") << QString("C:\\file.txt");
    QTest::newRow("control") << QString("bad\nname.txt");
    QTest::newRow("bidi-control") << QString("safe") + QChar(0x202e) + "txt.exe";
    QTest::newRow("trailing-dot") << QString("file.");
    QTest::newRow("trailing-space") << QString("file ");
    QTest::newRow("reserved") << QString("CON.txt");
    QTest::newRow("too-long") << QString(256, 'a');
}

void FileTransferPolicyTest::rejectsUnsafeFilenames()
{
    QFETCH(QString, filename);
    QVERIFY(!FileTransferPolicy::isSafeFilename(filename));
}

void FileTransferPolicyTest::validatesDeviceNames()
{
    QVERIFY(FileTransferPolicy::isSafeDeviceName("Living Room Mac"));
    QVERIFY(FileTransferPolicy::isSafeDeviceName("书房电脑"));
    QVERIFY(!FileTransferPolicy::isSafeDeviceName(QString()));
    QVERIFY(!FileTransferPolicy::isSafeDeviceName("device\nspoof"));
    QVERIFY(!FileTransferPolicy::isSafeDeviceName(QString("device") + QChar(0x202e)));
    QVERIFY(!FileTransferPolicy::isSafeDeviceName(QString(256, 'a')));
}

void FileTransferPolicyTest::validatesFileSizes()
{
    quint64 size = 99;
    QVERIFY(FileTransferPolicy::parseFileSize(0, &size));
    QCOMPARE(size, Q_UINT64_C(0));
    QVERIFY(FileTransferPolicy::parseFileSize(4096, &size));
    QCOMPARE(size, Q_UINT64_C(4096));
    QVERIFY(FileTransferPolicy::parseFileSize(
            static_cast<double>(FileTransferPolicy::maxFileSize()), &size));
    QCOMPARE(size, FileTransferPolicy::maxFileSize());

    QVERIFY(!FileTransferPolicy::parseFileSize(-1, &size));
    QVERIFY(!FileTransferPolicy::parseFileSize(1.5, &size));
    QVERIFY(!FileTransferPolicy::parseFileSize(
            static_cast<double>(FileTransferPolicy::maxFileSize()) + 1, &size));
    QVERIFY(!FileTransferPolicy::parseFileSize(1, nullptr));
}

void FileTransferPolicyTest::validatesPorts()
{
    quint16 port = 99;
    QVERIFY(FileTransferPolicy::parsePort(0, &port));
    QCOMPARE(port, quint16(0));
    QVERIFY(FileTransferPolicy::parsePort(1, &port));
    QCOMPARE(port, quint16(1));
    QVERIFY(FileTransferPolicy::parsePort(52637, &port));
    QCOMPARE(port, quint16(52637));
    QVERIFY(FileTransferPolicy::parsePort(65535, &port));
    QCOMPARE(port, quint16(65535));

    QVERIFY(!FileTransferPolicy::parsePort(-1, &port));
    QVERIFY(!FileTransferPolicy::parsePort(65536, &port));
    QVERIFY(!FileTransferPolicy::parsePort(70000, &port));
    QVERIFY(!FileTransferPolicy::parsePort(4.5, &port));
    QVERIFY(!FileTransferPolicy::parsePort(std::nan(""), &port));
    QVERIFY(!FileTransferPolicy::parsePort(std::numeric_limits<double>::infinity(), &port));
    QVERIFY(!FileTransferPolicy::parsePort(1, nullptr));
}

void FileTransferPolicyTest::enforcesTotalSize()
{
    QVERIFY(FileTransferPolicy::canAppendFile(0, FileTransferPolicy::maxFileSize()));
    QVERIFY(FileTransferPolicy::canAppendFile(
            FileTransferPolicy::maxTotalSize() - 1, 1));
    QVERIFY(!FileTransferPolicy::canAppendFile(
            FileTransferPolicy::maxTotalSize(), 1));
    QVERIFY(!FileTransferPolicy::canAppendFile(0, FileTransferPolicy::maxFileSize() + 1));
}

void FileTransferPolicyTest::generatesCollisionPaths()
{
    QTemporaryDir tempDir;
    QVERIFY(tempDir.isValid());

    QString original = FileTransferPolicy::destinationPath(tempDir.path(), "report.txt", 0);
    QCOMPARE(original, tempDir.filePath("report.txt"));
    QFile existing(original);
    QVERIFY(existing.open(QIODevice::WriteOnly));
    existing.close();

    QString duplicate = FileTransferPolicy::destinationPath(tempDir.path(), "report.txt", 1);
    QCOMPARE(duplicate, tempDir.filePath("report (1).txt"));
    QVERIFY(!QFileInfo::exists(duplicate));

    QString longName(251, 'a');
    longName += ".txt";
    QVERIFY(FileTransferPolicy::isSafeFilename(longName));
    QString longDuplicate = QFileInfo(
            FileTransferPolicy::destinationPath(tempDir.path(), longName, 9999)).fileName();
    QVERIFY(longDuplicate.toUtf8().size() <= FileTransferPolicy::MAX_FILENAME_BYTES);
}

void FileTransferPolicyTest::commitsWithoutOverwriting()
{
    QTemporaryDir tempDir;
    QVERIFY(tempDir.isValid());

    QFile original(tempDir.filePath("report.txt"));
    QVERIFY(original.open(QIODevice::WriteOnly));
    QCOMPARE(original.write("original"), Q_INT64_C(8));
    original.close();

    QTemporaryFile temporaryFile(tempDir.filePath(".part-XXXXXX"));
    QVERIFY(temporaryFile.open());
    QCOMPARE(temporaryFile.write("received"), Q_INT64_C(8));

    QString finalPath;
    QVERIFY(FileTransferPolicy::commitTemporaryFile(
            &temporaryFile, tempDir.path(), "report.txt", &finalPath));
    QCOMPARE(finalPath, tempDir.filePath("report (1).txt"));
    QVERIFY(!temporaryFile.autoRemove());

    QVERIFY(original.open(QIODevice::ReadOnly));
    QCOMPARE(original.readAll(), QByteArray("original"));
    original.close();

    QFile received(finalPath);
    QVERIFY(received.open(QIODevice::ReadOnly));
    QCOMPARE(received.readAll(), QByteArray("received"));
}

void FileTransferPolicyTest::exchangesEncryptedMessages()
{
    Crypto sender;
    Crypto receiver;
    sender.setRemotePublicKey(receiver.localPublicKey());
    receiver.setRemotePublicKey(sender.localPublicKey());

    QCOMPARE(sender.sessionKeyDigest(), receiver.sessionKeyDigest());
    QCOMPARE(receiver.decrypt(sender.encrypt(QByteArray())), QByteArray());
    QCOMPARE(receiver.decrypt(sender.encrypt("hello WireHop")), QByteArray("hello WireHop"));
    QCOMPARE(Crypto::encryptedOverhead(), Q_UINT64_C(28));
}

void FileTransferPolicyTest::rejectsInvalidCryptoInputs()
{
    Crypto sender;
    Crypto receiver;
    QVERIFY_EXCEPTION_THROWN(sender.setRemotePublicKey(QByteArray(31, 0)), std::runtime_error);
    QVERIFY_EXCEPTION_THROWN(sender.setRemotePublicKey(QByteArray(33, 0)), std::runtime_error);

    sender.setRemotePublicKey(receiver.localPublicKey());
    receiver.setRemotePublicKey(sender.localPublicKey());
    QByteArray cipherText = sender.encrypt("authenticated");

    QVERIFY_EXCEPTION_THROWN(
            receiver.decrypt(QByteArray(static_cast<int>(Crypto::encryptedOverhead()) - 1, 0)),
            std::runtime_error);
    int lastIndex = cipherText.size() - 1;
    cipherText[lastIndex] = static_cast<char>(cipherText.at(lastIndex) ^ 1);
    QVERIFY_EXCEPTION_THROWN(receiver.decrypt(cipherText), std::runtime_error);
}

int runFileTransferPolicyTest(int argc, char *argv[])
{
    FileTransferPolicyTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_filetransferpolicy.moc"
