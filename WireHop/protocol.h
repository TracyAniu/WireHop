// SPDX-License-Identifier: BSD-3-Clause

#pragma once

#include <QJsonObject>
#include <QJsonValue>
#include <QSet>
#include <QString>

// Wire-protocol version and capability negotiation shared by the transfer
// sessions and the discovery advertisement. See docs/references/PROTOCOL.md
// for the wire format and compatibility rules.
namespace Protocol {

enum {
    // Highest message-format version this build speaks. Version 0 is the
    // implicit LANDrop 0.4.0 wire format (no negotiation fields).
    VERSION = 1,
    // Bounds enforced on the untrusted peer capability list. The per-entry
    // bound is in UTF-8 bytes, as elsewhere on this trust boundary.
    MAX_CAPS = 32,
    MAX_CAP_BYTES = 32
};

// Capability identifiers. Values are wire format; keep them stable.
QString capAck();

// The capability set this build advertises.
QSet<QString> localCaps();

// Adds "protocol_version" and "caps" to a metadata, response, or discovery
// advertisement object. Purely additive: LANDrop 0.4.0 peers parse fixed
// keys and ignore these.
void insertNegotiationFields(QJsonObject &obj);

// Parses a peer's "protocol_version". Returns 0 (legacy) unless the value
// is a sane positive integer.
int parseVersion(const QJsonValue &value);

// Parses a peer's "caps" array with bounds enforcement. Any violation
// yields the legacy result: an empty set.
QSet<QString> parseCaps(const QJsonValue &value);

// --- Discovery datagrams (UDP 52637) ---------------------------------------
// See docs/references/PROTOCOL.md, "Discovery". These live here, rather than
// inside DiscoveryService, so they can be linked and tested without a GUI.

struct Advertisement {
    QString deviceName;
    QString deviceType;
    quint16 port = 0;
    int protocolVersion = 0;
    QSet<QString> caps;
};

enum DatagramKind {
    // Malformed, or failing the bounded validation: drop it.
    InvalidDatagram,
    // "Report in" — answer with a unicast advertisement to the source.
    DiscoveryRequest,
    // A peer describing itself. port == 0 means "I am not available".
    DiscoveryAdvertisement
};

// Classifies and validates one datagram payload. Fills *out only when the
// result is DiscoveryAdvertisement.
DatagramKind parseDiscoveryDatagram(const QByteArray &data, Advertisement *out);

QByteArray buildDiscoveryRequest();
QByteArray buildDiscoveryAdvertisement(const QString &deviceName, const QString &deviceType,
                                       quint16 port);

}
