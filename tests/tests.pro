QT += core testlib
QT -= gui

CONFIG += console testcase c++11
CONFIG -= app_bundle

TEMPLATE = app
TARGET = wirehop_tests

INCLUDEPATH += ../WireHop

SOURCES += \
    tst_filetransferpolicy.cpp \
    ../WireHop/crypto.cpp \
    ../WireHop/filetransferpolicy.cpp

HEADERS += \
    ../WireHop/crypto.h \
    ../WireHop/filetransferpolicy.h

unix {
    INCLUDEPATH += /usr/local/include
    LIBS += -L/usr/local/lib -lsodium
}
