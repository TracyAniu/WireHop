// SPDX-License-Identifier: BSD-3-Clause

#include <algorithm>

#include <QJsonArray>
#include <QStringList>

#include "protocol.h"

namespace Protocol {

QString capAck()
{
    return QStringLiteral("ack");
}

QSet<QString> localCaps()
{
    QSet<QString> caps;
    caps.insert(capAck());
    return caps;
}

void insertNegotiationFields(QJsonObject &obj)
{
    // Sorted so the serialized frame is byte-reproducible: QSet iteration
    // order varies per process (randomized qHash seed), and future work may
    // bind these bytes into a transcript digest.
    QStringList sorted;
    foreach (const QString &cap, localCaps())
        sorted.append(cap);
    std::sort(sorted.begin(), sorted.end());

    QJsonArray caps;
    foreach (const QString &cap, sorted)
        caps.append(cap);
    obj.insert("protocol_version", static_cast<int>(VERSION));
    obj.insert("caps", caps);
}

int parseVersion(const QJsonValue &value)
{
    if (!value.isDouble())
        return 0;
    double version = value.toDouble();
    if (version < 1.0 || version > 1000000.0
            || version != static_cast<double>(static_cast<int>(version)))
        return 0;
    return static_cast<int>(version);
}

QSet<QString> parseCaps(const QJsonValue &value)
{
    if (!value.isArray())
        return QSet<QString>();

    QJsonArray array = value.toArray();
    if (array.size() > MAX_CAPS)
        return QSet<QString>();

    QSet<QString> caps;
    foreach (const QJsonValue &v, array) {
        if (!v.isString())
            return QSet<QString>();
        QString cap = v.toString();
        // Bounded in UTF-8 bytes, matching FileTransferPolicy's convention on
        // this trust boundary: QString::size() counts UTF-16 code units, which
        // would admit up to 4x the intended byte length.
        int capBytes = cap.toUtf8().size();
        if (capBytes == 0 || capBytes > MAX_CAP_BYTES)
            return QSet<QString>();
        caps.insert(cap);
    }
    return caps;
}

}
