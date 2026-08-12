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
    // Bounds enforced on the untrusted peer capability list.
    MAX_CAPS = 32,
    MAX_CAP_LENGTH = 32
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

}
