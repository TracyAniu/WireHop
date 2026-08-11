QT += core testlib
QT -= gui

CONFIG += console testcase c++11
CONFIG -= app_bundle

TEMPLATE = app
TARGET = landrop_tests

INCLUDEPATH += ../LANDrop

SOURCES += \
    tst_filetransferpolicy.cpp \
    ../LANDrop/crypto.cpp \
    ../LANDrop/filetransferpolicy.cpp

HEADERS += \
    ../LANDrop/crypto.h \
    ../LANDrop/filetransferpolicy.h

unix {
    INCLUDEPATH += /usr/local/include
    LIBS += -L/usr/local/lib -lsodium
}
