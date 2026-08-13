// SPDX-License-Identifier: BSD-3-Clause

#pragma once

#include <QString>
#include <QtGlobal>

class QTemporaryFile;

class FileTransferPolicy {
public:
    enum {
        MAX_FILES_PER_TRANSFER = 1024,
        MAX_FILENAME_BYTES = 255,
        MAX_DEVICE_NAME_BYTES = 255,
        MAX_COLLISION_ATTEMPTS = 10000,
        MAX_DISCOVERY_DATAGRAM_BYTES = 4096
    };

    static quint64 maxFileSize();
    static quint64 maxTotalSize();
    static bool isSafeFilename(const QString &filename);
    static bool isSafeDeviceName(const QString &deviceName);
    static bool parseFileSize(double value, quint64 *size);
    static bool parsePort(double value, quint16 *port);
    static bool canAppendFile(quint64 currentTotal, quint64 fileSize);
    static QString destinationPath(const QString &downloadPath, const QString &filename,
                                   int duplicateIndex);
    static bool commitTemporaryFile(QTemporaryFile *temporaryFile, const QString &downloadPath,
                                    const QString &filename, QString *finalPath = nullptr);
};
