// SPDX-License-Identifier: BSD-3-Clause

#include <algorithm>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>
#include <QtTest>

#include "protocol.h"

class ProtocolTest : public QObject {
    Q_OBJECT
private slots:
    void versionParsesSanePositiveIntegers();
    void versionRejectsMalformedValues();
    void capsParseBoundedStringArrays();
    void capsRejectMalformedArrays();
    void capsAcceptMaximumByteLengthEntry();
    void negotiationFieldsRoundTrip();
    void negotiationFieldOrderIsDeterministic();
};

void ProtocolTest::versionParsesSanePositiveIntegers()
{
    QCOMPARE(Protocol::parseVersion(QJsonValue(1)), 1);
    QCOMPARE(Protocol::parseVersion(QJsonValue(7)), 7);
}

void ProtocolTest::versionRejectsMalformedValues()
{
    QCOMPARE(Protocol::parseVersion(QJsonValue()), 0);        // absent
    QCOMPARE(Protocol::parseVersion(QJsonValue("1")), 0);     // wrong type
    QCOMPARE(Protocol::parseVersion(QJsonValue(true)), 0);    // wrong type
    QCOMPARE(Protocol::parseVersion(QJsonValue(0)), 0);
    QCOMPARE(Protocol::parseVersion(QJsonValue(-3)), 0);
    QCOMPARE(Protocol::parseVersion(QJsonValue(1.5)), 0);     // non-integer
    QCOMPARE(Protocol::parseVersion(QJsonValue(1e9)), 0);     // out of bounds
}

void ProtocolTest::capsParseBoundedStringArrays()
{
    QJsonArray array;
    array.append("ack");
    array.append("resume");
    QSet<QString> caps = Protocol::parseCaps(array);
    QCOMPARE(caps.size(), 2);
    QVERIFY(caps.contains("ack"));
    QVERIFY(caps.contains("resume"));
}

void ProtocolTest::capsRejectMalformedArrays()
{
    QVERIFY(Protocol::parseCaps(QJsonValue()).isEmpty());      // absent
    QVERIFY(Protocol::parseCaps(QJsonValue("ack")).isEmpty()); // wrong type

    QJsonArray tooMany;
    for (int i = 0; i < Protocol::MAX_CAPS + 1; ++i)
        tooMany.append(QString("cap%1").arg(i));
    QVERIFY(Protocol::parseCaps(tooMany).isEmpty());

    QJsonArray nonString;
    nonString.append("ack");
    nonString.append(42);
    QVERIFY(Protocol::parseCaps(nonString).isEmpty());

    QJsonArray oversizedEntry;
    oversizedEntry.append(QString(Protocol::MAX_CAP_BYTES + 1, QLatin1Char('a')));
    QVERIFY(Protocol::parseCaps(oversizedEntry).isEmpty());

    QJsonArray emptyEntry;
    emptyEntry.append("");
    QVERIFY(Protocol::parseCaps(emptyEntry).isEmpty());

    // The bound is UTF-8 bytes, not UTF-16 code units: 16 CJK characters fit
    // QString::size() <= 32 but carry 48 bytes.
    QJsonArray multiByteEntry;
    multiByteEntry.append(QString(16, QChar(0x4E2D)));
    QVERIFY(Protocol::parseCaps(multiByteEntry).isEmpty());
}

void ProtocolTest::capsAcceptMaximumByteLengthEntry()
{
    QJsonArray atBound;
    atBound.append(QString(Protocol::MAX_CAP_BYTES, QLatin1Char('a')));
    QCOMPARE(Protocol::parseCaps(atBound).size(), 1);
}

void ProtocolTest::negotiationFieldOrderIsDeterministic()
{
    // The serialized capability array must be byte-reproducible across
    // processes; QSet iteration order is not.
    QJsonObject obj;
    Protocol::insertNegotiationFields(obj);
    QJsonArray caps = obj.value("caps").toArray();

    QStringList emitted;
    foreach (const QJsonValue &v, caps)
        emitted.append(v.toString());

    QStringList sorted = emitted;
    std::sort(sorted.begin(), sorted.end());
    QCOMPARE(emitted, sorted);
}

void ProtocolTest::negotiationFieldsRoundTrip()
{
    QJsonObject obj;
    Protocol::insertNegotiationFields(obj);
    QCOMPARE(Protocol::parseVersion(obj.value("protocol_version")),
             static_cast<int>(Protocol::VERSION));
    QSet<QString> caps = Protocol::parseCaps(obj.value("caps"));
    QCOMPARE(caps, Protocol::localCaps());
    QVERIFY(caps.contains(Protocol::capAck()));
}

int runProtocolTest(int argc, char *argv[])
{
    ProtocolTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_protocol.moc"
