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

#include <QDesktopServices>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStorageInfo>
#include <QTimer>
#include <QUrl>

#include "filetransferpolicy.h"
#include "filetransferreceiver.h"
#include "settings.h"

FileTransferReceiver::FileTransferReceiver(QObject *parent, QTcpSocket *socket) :
    FileTransferSession(parent, socket), writingFile(nullptr), downloadPath(Settings::downloadPath()) {}

void FileTransferReceiver::respond(bool accepted)
{
    if (state != AWAITING_RESPONSE) {
        emit errorOccurred(tr("Handshake failed."));
        return;
    }

    if (accepted) {
        if (!QDir().mkpath(downloadPath)) {
            emit errorOccurred(tr("Cannot create download path: ") + downloadPath);
            return;
        }
        if (!QFileInfo(downloadPath).isWritable()) {
            emit errorOccurred(tr("Download path is not writable: ") + downloadPath);
            return;
        }
        QStorageInfo storage(downloadPath);
        qint64 availableBytes = storage.bytesAvailable();
        if (storage.isValid() && storage.isReady() && availableBytes >= 0
                && totalSize > static_cast<quint64>(availableBytes)) {
            emit errorOccurred(tr("Not enough free space in the download path."));
            return;
        }
    }

    QJsonObject obj;
    obj.insert("response", static_cast<int>(accepted));
    if (!encryptAndSend(QJsonDocument(obj).toJson(QJsonDocument::Compact)))
        return;

    if (accepted) {
        state = TRANSFERRING;
        createNextFile();
    } else {
        state = FINISHED;
        connect(socket, &QTcpSocket::bytesWritten, this, &FileTransferReceiver::ended);
    }
}

void FileTransferReceiver::processReceivedData(const QByteArray &data)
{
    if (state == HANDSHAKE2) {
        QJsonParseError parseError;
        QJsonDocument json = QJsonDocument::fromJson(data, &parseError);
        if (!json.isObject()) {
            emit errorOccurred(tr("Invalid file metadata."));
            return;
        }

        QJsonObject obj = json.object();
        QJsonValue deviceName = obj.value("device_name");
        if (!deviceName.isString() || !FileTransferPolicy::isSafeDeviceName(deviceName.toString())) {
            emit errorOccurred(tr("Invalid sender name."));
            return;
        }

        QJsonValue filesJson = obj.value("files");
        if (!filesJson.isArray()) {
            emit errorOccurred(tr("Invalid file metadata."));
            return;
        }

        QJsonArray filesJsonArray = filesJson.toArray();
        if (filesJsonArray.empty()) {
            emit errorOccurred(tr("No files were offered."));
            return;
        }
        if (filesJsonArray.size() > FileTransferPolicy::MAX_FILES_PER_TRANSFER) {
            emit errorOccurred(tr("Too many files were offered."));
            return;
        }

        QList<FileMetadata> metadata;
        quint64 declaredTotalSize = 0;
        foreach (const QJsonValue &v, filesJsonArray) {
            if (!v.isObject()) {
                emit errorOccurred(tr("Invalid file metadata."));
                return;
            }
            QJsonObject o = v.toObject();

            QJsonValue filename = o.value("filename");
            if (!filename.isString() || !FileTransferPolicy::isSafeFilename(filename.toString())) {
                emit errorOccurred(tr("Unsafe filename was rejected."));
                return;
            }

            QJsonValue size = o.value("size");
            quint64 sizeInt;
            if (!size.isDouble() || !FileTransferPolicy::parseFileSize(size.toDouble(), &sizeInt)) {
                emit errorOccurred(tr("Invalid or oversized file was rejected."));
                return;
            }
            if (!FileTransferPolicy::canAppendFile(declaredTotalSize, sizeInt)) {
                emit errorOccurred(tr("The total transfer size is too large."));
                return;
            }

            declaredTotalSize += sizeInt;
            metadata.append({filename.toString(), sizeInt});
        }

        transferQ = metadata;
        totalSize = declaredTotalSize;
        state = AWAITING_RESPONSE;
        emit fileMetadataReady(transferQ, totalSize, deviceName.toString(),
                               crypto.sessionKeyDigest());
    } else if (state == AWAITING_RESPONSE) {
        emit errorOccurred(tr("Handshake failed."));
    } else if (state == TRANSFERRING) {
        if (transferredSize > totalSize
                || static_cast<quint64>(data.size()) > totalSize - transferredSize) {
            emit errorOccurred(tr("Received more file data than declared."));
            return;
        }

        QByteArray tmpData = data;
        while (tmpData.size() > 0) {
            if (transferQ.empty() || !writingFile) {
                emit errorOccurred(tr("Received more file data than declared."));
                return;
            }

            FileMetadata &curFile = transferQ.first();
            quint64 writeSize = qMin(curFile.size, static_cast<quint64>(tmpData.size()));
            qint64 written = writingFile->write(tmpData.constData(), static_cast<qint64>(writeSize));
            if (written <= 0) {
                emit errorOccurred(tr("Unable to write received file."));
                return;
            }

            curFile.size -= static_cast<quint64>(written);
            transferredSize += static_cast<quint64>(written);
            tmpData.remove(0, static_cast<int>(written));
            if (totalSize > 0)
                emit updateProgress(static_cast<double>(transferredSize) / totalSize);

            if (curFile.size == 0) {
                QString filename = curFile.filename;
                if (!finalizeCurrentFile(filename))
                    return;
                transferQ.pop_front();
                if (transferQ.empty() && !tmpData.isEmpty()) {
                    emit errorOccurred(tr("Received more file data than declared."));
                    return;
                }
                createNextFile();
            }
        }
    }
}

bool FileTransferReceiver::createCurrentTempFile()
{
    writingFile = new QTemporaryFile(QDir(downloadPath).filePath(".wirehop-part-XXXXXX"), this);
    writingFile->setAutoRemove(true);
    if (!writingFile->open()) {
        writingFile->deleteLater();
        writingFile = nullptr;
        emit errorOccurred(tr("Unable to create a temporary file in %1.").arg(downloadPath));
        return false;
    }
    return true;
}

bool FileTransferReceiver::finalizeCurrentFile(const QString &filename)
{
    if (!writingFile)
        return false;
    if (!FileTransferPolicy::commitTemporaryFile(writingFile, downloadPath, filename)) {
        emit errorOccurred(tr("Unable to finalize received file %1.").arg(filename));
        return false;
    }

    writingFile->deleteLater();
    writingFile = nullptr;
    return true;
}

void FileTransferReceiver::createNextFile()
{
    while (!transferQ.empty()) {
        FileMetadata &curFile = transferQ.first();
        if (!createCurrentTempFile())
            return;
        if (curFile.size > 0) {
            emit printMessage(tr("Receiving file %1...").arg(curFile.filename));
            break;
        }
        QString filename = curFile.filename;
        if (!finalizeCurrentFile(filename))
            return;
        transferQ.pop_front();
    }
    if (transferQ.empty()) {
        state = FINISHED;
        QDesktopServices::openUrl(QUrl::fromLocalFile(downloadPath));
        emit printMessage(tr("Done!"));
        socket->disconnectFromHost();
        QTimer::singleShot(5000, this, &FileTransferSession::ended);
    }
}
