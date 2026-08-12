// SPDX-License-Identifier: BSD-3-Clause

#include <algorithm>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStringList>

#include "filetransferpolicy.h"
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

DatagramKind parseDiscoveryDatagram(const QByteArray &data, Advertisement *out)
{
    // Bounded here as well as at the socket, so every caller of this parser
    // inherits the limit rather than having to remember it. DiscoveryService
    // still checks first to avoid buffering an oversized datagram at all.
    if (data.isEmpty() || data.size() > FileTransferPolicy::MAX_DISCOVERY_DATAGRAM_BYTES)
        return InvalidDatagram;

    QJsonDocument json = QJsonDocument::fromJson(data);
    if (!json.isObject())
        return InvalidDatagram;
    QJsonObject obj = json.object();

    // "request" is the discriminator and must be a real boolean; a missing or
    // mistyped value is not a request and not an advertisement.
    QJsonValue request = obj.value("request");
    if (!request.isBool())
        return InvalidDatagram;
    if (request.toBool())
        return DiscoveryRequest;

    QJsonValue deviceName = obj.value("device_name");
    QJsonValue remotePort = obj.value("port");
    if (!deviceName.isString() || !remotePort.isDouble())
        return InvalidDatagram;

    quint16 port;
    if (!FileTransferPolicy::parsePort(remotePort.toDouble(), &port))
        return InvalidDatagram;
    QString deviceNameStr = deviceName.toString();
    if (!FileTransferPolicy::isSafeDeviceName(deviceNameStr))
        return InvalidDatagram;

    if (out) {
        out->deviceName = deviceNameStr;
        out->deviceType = obj.value("device_type").toString();
        out->port = port;
        // Untrusted hints; recorded but never used to gate behavior.
        out->protocolVersion = parseVersion(obj.value("protocol_version"));
        out->caps = parseCaps(obj.value("caps"));
    }
    return DiscoveryAdvertisement;
}

QByteArray buildDiscoveryRequest()
{
    QJsonObject obj;
    obj.insert("request", true);
    return QJsonDocument(obj).toJson(QJsonDocument::Compact);
}

QByteArray buildDiscoveryAdvertisement(const QString &deviceName, const QString &deviceType,
                                       quint16 port)
{
    QJsonObject obj;
    obj.insert("request", false);
    obj.insert("device_name", deviceName);
    obj.insert("device_type", deviceType);
    obj.insert("port", static_cast<int>(port));
    // Untrusted hint for peer-list use; the authoritative negotiation happens
    // inside the encrypted transfer session.
    insertNegotiationFields(obj);
    return QJsonDocument(obj).toJson(QJsonDocument::Compact);
}

}
