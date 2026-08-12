QT += core gui widgets network

CONFIG += c++11

TARGET = WireHop

SOURCES += \
    aboutdialog.cpp \
    crypto.cpp \
    discoveryservice.cpp \
    filetransferdialog.cpp \
    filetransferpolicy.cpp \
    filetransferreceiver.cpp \
    filetransfersender.cpp \
    filetransferserver.cpp \
    filetransfersession.cpp \
    main.cpp \
    selectfilesdialog.cpp \
    sendtodialog.cpp \
    settings.cpp \
    settingsdialog.cpp \
    trayicon.cpp

HEADERS += \
    aboutdialog.h \
    crypto.h \
    discoveryservice.h \
    filetransferdialog.h \
    filetransferpolicy.h \
    filetransferreceiver.h \
    filetransfersender.h \
    filetransferserver.h \
    filetransfersession.h \
    selectfilesdialog.h \
    sendtodialog.h \
    settings.h \
    settingsdialog.h \
    trayicon.h

FORMS += \
    aboutdialog.ui \
    filetransferdialog.ui \
    selectfilesdialog.ui \
    sendtodialog.ui \
    settingsdialog.ui

RESOURCES += \
    icons.qrc \
    locales.qrc

TRANSLATIONS += \
    locales/WireHop.zh_CN.ts

RC_ICONS = icons/app.ico
ICON = icons/app.icns

unix {
    INCLUDEPATH += /usr/local/include
    LIBS += -L/usr/local/lib -lsodium

    PREFIX = $$(PREFIX)
    isEmpty(PREFIX) {
        PREFIX = /usr/local
    }

    binary.path = $$PREFIX/bin
    binary.files = $$OUT_PWD/wirehop
    binary.extra = cp "$$OUT_PWD/WireHop" "$$OUT_PWD/wirehop"
    binary.CONFIG = no_check_exist executable

    icon.path = $$PREFIX/share/icons/hicolor/scalable/apps
    icon.files = $$OUT_PWD/wirehop.svg
    icon.extra = cp "$$PWD/icons/app.svg" "$$OUT_PWD/wirehop.svg"
    icon.CONFIG = no_check_exist 

    desktop.path = $$PREFIX/share/applications
    desktop.files = $$OUT_PWD/wirehop.desktop
    desktop.extra = cp "$$PWD/../misc/WireHop.desktop" "$$OUT_PWD/wirehop.desktop"
    desktop.CONFIG = no_check_exist 

    INSTALLS += binary icon desktop
}

QMAKE_INFO_PLIST = Info.plist

macx {
    HEADERS += macservices.h
    OBJECTIVE_SOURCES += macservices.mm
    LIBS += -framework AppKit

    infoplist_locales.files = locales/zh-Hans.lproj
    infoplist_locales.path = Contents/Resources
    QMAKE_BUNDLE_DATA += infoplist_locales

    # Embed the Share-sheet extension after linking (see shareext/).
    QMAKE_POST_LINK += $$shell_quote($$PWD/../scripts/build-share-extension.sh) $$shell_quote($$OUT_PWD/WireHop.app)
}
