// SPDX-License-Identifier: BSD-3-Clause

#pragma once

class TrayIcon;

// Registers the macOS Services provider backing the "Send with WireHop"
// Finder context-menu entry declared under NSServices in Info.plist.
void registerMacServicesProvider(TrayIcon *trayIcon);
