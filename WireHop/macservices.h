// SPDX-License-Identifier: BSD-3-Clause

#pragma once

class TrayIcon;

// Registers the macOS Services provider backing the "Send with WireHop"
// Finder context-menu entry declared under NSServices in Info.plist.
void registerMacServicesProvider(TrayIcon *trayIcon);

// Brings this application to the front.
//
// LSUIElement accessory applications are outside the normal activation order,
// so QWidget::activateWindow() only makes a window key -- the application
// itself stays behind whatever the user was using, leaving a fully drawn
// dialog invisible underneath it. Every external intake path (Share sheet,
// Services, open-with, command line) needs this to surface its dialog.
void activateApplication();
