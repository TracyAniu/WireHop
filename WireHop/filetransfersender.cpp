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

#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTimer>

#include "filetransfersender.h"

#include "filetransferpolicy.h"
#include "protocol.h"

FileTransferSender::FileTransferSender(QObject *parent, QTcpSocket *socket, const QList<QSharedPointer<QFile>> &files,
                                       const QString &deviceName) :
    FileTransferSession(parent, socket), files(files), deviceName(deviceName)
{
    connect(socket, &QTcpSocket::bytesWritten, this, &FileTransferSender::socketBytesWritten);

    foreach (QSharedPointer<QFile> file, files) {
        QString filename = QFileInfo(*file).fileName();
        quint64 size = static_cast<quint64>(file->size());
        totalSize += size;
        transferQ.append({filename, size});
    }
}

int FileTransferSender::watchdogIntervalMsecs() const
{
    // The sender idles in HANDSHAKE2 while the receiving user decides, so it
    // gets the same human-scale budget as the receiver's AWAITING_RESPONSE.
    if (state == HANDSHAKE2)
        return RESPONSE_TIMEOUT_MSECS;
    // Only peers that negotiated the "ack" capability earn the full
    // acknowledgment window; see ACK_GRACE_TIMEOUT_MSECS.
    if (state == WAITING_FOR_ACK && !hasNegotiatedCap(Protocol::capAck()))
        return ACK_GRACE_TIMEOUT_MSECS;
    return FileTransferSession::watchdogIntervalMsecs();
}

void FileTransferSender::watchdogTimedOut()
{
    if (state == WAITING_FOR_ACK) {
        finishUnconfirmed();
        return;
    }
    FileTransferSession::watchdogTimedOut();
}

void FileTransferSender::handleSocketError()
{
    // Peers without the completion acknowledgment close the connection right
    // after the last byte; that is qualified success, not an error.
    if (state == WAITING_FOR_ACK) {
        finishUnconfirmed();
        return;
    }
    FileTransferSession::handleSocketError();
}

void FileTransferSender::finishConfirmed()
{
    state = FINISHED;
    touchWatchdog();
    emit printMessage(tr("Done!"));
    socket->disconnectFromHost();
    QTimer::singleShot(5000, this, &FileTransferSession::ended);
}

void FileTransferSender::finishUnconfirmed()
{
    state = FINISHED;
    touchWatchdog();
    emit printMessage(tr("Sent, but the receiver did not confirm delivery."));
    socket->abort();
    QTimer::singleShot(5000, this, &FileTransferSession::ended);
}

void FileTransferSender::handshake1Finished()
{
    if (transferQ.isEmpty() || transferQ.size() > FileTransferPolicy::MAX_FILES_PER_TRANSFER) {
        emit errorOccurred(tr("The selected file count is invalid."));
        return;
    }

    if (!FileTransferPolicy::isSafeDeviceName(deviceName)) {
        emit errorOccurred(tr("The configured device name is invalid."));
        return;
    }

    quint64 validatedTotalSize = 0;
    QJsonArray jsonFiles;
    foreach (FileMetadata metadata, transferQ) {
        if (!FileTransferPolicy::isSafeFilename(metadata.filename)
                || !FileTransferPolicy::canAppendFile(validatedTotalSize, metadata.size)) {
            emit errorOccurred(tr("A selected file has an unsafe name or unsupported size."));
            return;
        }
        validatedTotalSize += metadata.size;

        QJsonObject jsonFile;
        jsonFile.insert("filename", metadata.filename);
        jsonFile.insert("size", static_cast<qint64>(metadata.size));
        jsonFiles.append(jsonFile);
    }

    QJsonObject obj;
    obj.insert("device_name", deviceName);
    obj.insert("device_type", QSysInfo::productType());
    obj.insert("files", jsonFiles);
    Protocol::insertNegotiationFields(obj);
    totalSize = validatedTotalSize;
    encryptAndSend(QJsonDocument(obj).toJson(QJsonDocument::Compact));
}

void FileTransferSender::processReceivedData(const QByteArray &data)
{
    if (state == HANDSHAKE2) {
        QJsonDocument json = QJsonDocument::fromJson(data);
        if (!json.isObject()) {
            emit errorOccurred(tr("Handshake failed."));
            return;
        }

        QJsonObject obj = json.object();
        QJsonValue response = obj.value("response");
        if (!response.isDouble()
                || (response.toDouble() != 0.0 && response.toDouble() != 1.0)) {
            emit errorOccurred(tr("Handshake failed."));
            return;
        }

        adoptPeerNegotiation(obj);

        if (response.toInt() == 0) {
            // Terminal state first: a peer that closes immediately after
            // declining would otherwise re-enter handleSocketError() and
            // replace the reason the user needs with "the remote host closed
            // the connection". Nothing obliges a peer to linger after a
            // rejection, and the Rust core does not.
            state = FINISHED;
            emit errorOccurred(tr("The receiving device rejected your file(s)."));
            return;
        }
        state = TRANSFERRING;
        socketBytesWritten();
    } else if (state == WAITING_FOR_ACK) {
        QJsonDocument json = QJsonDocument::fromJson(data);
        if (!json.isObject())
            return;
        if (json.object().value("ack").toDouble() == 1.0)
            finishConfirmed();
    }
}

void FileTransferSender::socketBytesWritten()
{
    if (state != TRANSFERRING || socket->bytesToWrite() > 0)
        return;

    while (!transferQ.empty()) {
        FileMetadata &curFile = transferQ.front();
        if (curFile.size == 0) {
            transferQ.pop_front();
            files.pop_front();
        } else {
            emit printMessage(tr("Sending file %1...").arg(curFile.filename));
            break;
        }
    }
    if (transferQ.empty()) {
        state = WAITING_FOR_ACK;
        touchWatchdog();
        emit printMessage(tr("Waiting for the receiver to confirm..."));
        return;
    }
    QSharedPointer<QFile> &curFile = files.front();
    FileMetadata &curMetadata = transferQ.front();
    qint64 readSize = static_cast<qint64>(qMin<quint64>(TRANSFER_QUANTA, curMetadata.size));
    QByteArray data = curFile->read(readSize);
    if (data.isEmpty() && curMetadata.size > 0) {
        emit errorOccurred(tr("Unable to read file %1.").arg(curMetadata.filename));
        return;
    }
    if (!encryptAndSend(data))
        return;
    curMetadata.size -= static_cast<quint64>(data.size());
    transferredSize += static_cast<quint64>(data.size());
    emit updateProgress(static_cast<double>(transferredSize) / totalSize);
    touchWatchdog();
}
