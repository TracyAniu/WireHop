// SPDX-License-Identifier: BSD-3-Clause

#include <cmath>

#include <QDir>
#include <QFileInfo>
#include <QSet>
#include <QTemporaryFile>

#include "filetransferpolicy.h"

namespace {

QString truncateUtf8(QString value, int maxBytes)
{
    QString result;
    foreach (uint codePoint, value.toUcs4()) {
        QString character = QString::fromUcs4(&codePoint, 1);
        if ((result + character).toUtf8().size() > maxBytes)
            break;
        result += character;
    }
    return result;
}

bool isReservedWindowsName(const QString &filename)
{
    QString basename = filename.section('.', 0, 0).toUpper();
    static const QSet<QString> reservedNames = {
        "CON", "PRN", "AUX", "NUL", "CLOCK$",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"
    };
    return reservedNames.contains(basename);
}

bool containsUnsafeUnicode(const QString &value)
{
    foreach (uint codePoint, value.toUcs4()) {
        QChar::Category category = QChar::category(codePoint);
        if (codePoint < 32 || codePoint == 127 || category == QChar::Other_Control
                || category == QChar::Other_Format || category == QChar::Other_Surrogate
                || category == QChar::Other_NotAssigned || category == QChar::Separator_Line
                || category == QChar::Separator_Paragraph)
            return true;
    }
    return false;
}

}

quint64 FileTransferPolicy::maxFileSize()
{
    return Q_UINT64_C(1024) * 1024 * 1024 * 1024;
}

quint64 FileTransferPolicy::maxTotalSize()
{
    return maxFileSize() * 4;
}

bool FileTransferPolicy::isSafeFilename(const QString &filename)
{
    if (filename.isEmpty() || filename == "." || filename == "..")
        return false;
    if (filename.toUtf8().size() > MAX_FILENAME_BYTES)
        return false;
    if (QFileInfo(filename).isAbsolute() || filename.contains('/') || filename.contains('\\'))
        return false;
    if (filename.endsWith('.') || filename.endsWith(' '))
        return false;

    const QString forbiddenCharacters = "<>:\"|?*";
    if (containsUnsafeUnicode(filename))
        return false;
    foreach (QChar character, filename)
        if (forbiddenCharacters.contains(character))
            return false;

    return !isReservedWindowsName(filename);
}

bool FileTransferPolicy::isSafeDeviceName(const QString &deviceName)
{
    if (deviceName.isEmpty() || deviceName.toUtf8().size() > MAX_DEVICE_NAME_BYTES)
        return false;
    return !containsUnsafeUnicode(deviceName);
}

bool FileTransferPolicy::parseFileSize(double value, quint64 *size)
{
    if (!size || !std::isfinite(value) || value < 0 || std::floor(value) != value
            || value > static_cast<double>(maxFileSize()))
        return false;

    *size = static_cast<quint64>(value);
    return true;
}

bool FileTransferPolicy::canAppendFile(quint64 currentTotal, quint64 fileSize)
{
    return fileSize <= maxFileSize() && currentTotal <= maxTotalSize()
            && fileSize <= maxTotalSize() - currentTotal;
}

QString FileTransferPolicy::destinationPath(const QString &downloadPath, const QString &filename,
                                            int duplicateIndex)
{
    if (!isSafeFilename(filename) || duplicateIndex < 0
            || duplicateIndex >= MAX_COLLISION_ATTEMPTS)
        return QString();

    if (duplicateIndex == 0)
        return QDir(downloadPath).filePath(filename);

    int suffixIndex = filename.lastIndexOf('.');
    QString base = suffixIndex > 0 ? filename.left(suffixIndex) : filename;
    QString extension = suffixIndex > 0 ? filename.mid(suffixIndex) : QString();
    QString marker = QString(" (%1)").arg(duplicateIndex);
    int maxBaseBytes = MAX_FILENAME_BYTES - extension.toUtf8().size() - marker.toUtf8().size();
    if (maxBaseBytes < 1) {
        QString shortened = truncateUtf8(filename, MAX_FILENAME_BYTES - marker.toUtf8().size());
        return QDir(downloadPath).filePath(shortened + marker);
    }
    base = truncateUtf8(base, maxBaseBytes);
    if (base.isEmpty())
        base = "file";

    return QDir(downloadPath).filePath(base + marker + extension);
}

bool FileTransferPolicy::commitTemporaryFile(QTemporaryFile *temporaryFile,
                                             const QString &downloadPath,
                                             const QString &filename, QString *finalPath)
{
    if (!temporaryFile || !temporaryFile->isOpen() || !isSafeFilename(filename))
        return false;
    if (!temporaryFile->flush())
        return false;
    temporaryFile->close();

    for (int duplicateIndex = 0; duplicateIndex < MAX_COLLISION_ATTEMPTS; ++duplicateIndex) {
        QString destination = destinationPath(downloadPath, filename, duplicateIndex);
        if (destination.isEmpty())
            return false;
        if (QFileInfo::exists(destination))
            continue;
        if (temporaryFile->rename(destination)) {
            temporaryFile->setAutoRemove(false);
            if (finalPath)
                *finalPath = destination;
            return true;
        }
        if (!QFileInfo::exists(destination))
            return false;
    }

    return false;
}
