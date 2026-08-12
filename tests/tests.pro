QT += core network testlib
QT -= gui

CONFIG += console testcase c++11
CONFIG -= app_bundle

TEMPLATE = app
TARGET = wirehop_tests

INCLUDEPATH += ../WireHop

SOURCES += \
    main.cpp \
    tst_filetransferpolicy.cpp \
    tst_filetransfersession.cpp \
    tst_protocol.cpp \
    ../WireHop/crypto.cpp \
    ../WireHop/filetransferpolicy.cpp \
    ../WireHop/filetransferreceiver.cpp \
    ../WireHop/filetransfersender.cpp \
    ../WireHop/filetransfersession.cpp \
    ../WireHop/protocol.cpp

HEADERS += \
    ../WireHop/crypto.h \
    ../WireHop/filetransferpolicy.h \
    ../WireHop/filetransferreceiver.h \
    ../WireHop/filetransfersender.h \
    ../WireHop/filetransfersession.h \
    ../WireHop/protocol.h

unix {
    INCLUDEPATH += /usr/local/include
    LIBS += -L/usr/local/lib -lsodium
}
