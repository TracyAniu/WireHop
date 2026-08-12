// SPDX-License-Identifier: BSD-3-Clause

#include <QCoreApplication>

int runFileTransferPolicyTest(int argc, char *argv[]);
int runFileTransferSessionTest(int argc, char *argv[]);
int runProtocolTest(int argc, char *argv[]);
int runProtocolVectorsTest(int argc, char *argv[]);

int main(int argc, char *argv[])
{
    QCoreApplication app(argc, argv);
    int status = 0;
    status |= runFileTransferPolicyTest(argc, argv);
    status |= runFileTransferSessionTest(argc, argv);
    status |= runProtocolTest(argc, argv);
    status |= runProtocolVectorsTest(argc, argv);
    return status;
}
