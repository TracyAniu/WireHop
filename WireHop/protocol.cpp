// SPDX-License-Identifier: BSD-3-Clause

#include <QJsonArray>

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
    QJsonArray caps;
    foreach (const QString &cap, localCaps())
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
        if (cap.isEmpty() || cap.size() > MAX_CAP_LENGTH)
            return QSet<QString>();
        caps.insert(cap);
    }
    return caps;
}

}
